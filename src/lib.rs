// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-omarchy-bluetooth — bluetoothctl-style Bluetooth manager.
//!
//! Models the local adapter plus discovered/paired devices with a real
//! state machine (discovered -> paired -> bonded -> connected -> trusted).
//! Deterministic in-memory core; a BlueZ/D-Bus backend can drive the same API.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceKind {
    Classic,
    Le,
    Dual,
}

impl fmt::Display for DeviceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Classic => write!(f, "classic"),
            Self::Le => write!(f, "le"),
            Self::Dual => write!(f, "dual"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceState {
    Discovered,
    Paired,
    Bonded,
    Connected,
    Trusted,
}

impl DeviceState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Paired => "paired",
            Self::Bonded => "bonded",
            Self::Connected => "connected",
            Self::Trusted => "trusted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct BluetoothDevice {
    pub mac: String,
    pub name: String,
    pub alias: Option<String>,
    pub kind: DeviceKind,
    pub rssi: i16,
    pub state: DeviceState,
    pub services: Vec<String>,
}

impl BluetoothDevice {
    pub fn new(mac: impl Into<String>, name: impl Into<String>, kind: DeviceKind) -> Self {
        Self {
            mac: mac.into(),
            name: name.into(),
            alias: None,
            kind,
            rssi: -60,
            state: DeviceState::Discovered,
            services: Vec::new(),
        }
    }
    pub fn display_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothError {
    EmptyMac,
    DuplicateMac(String),
    NoAdapter,
    NoSuchDevice(String),
    BadState { mac: String, want: &'static str, got: &'static str },
    AdapterOff,
}

impl fmt::Display for BluetoothError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMac => write!(f, "device MAC must not be empty"),
            Self::DuplicateMac(m) => write!(f, "duplicate device {m}"),
            Self::NoAdapter => write!(f, "no bluetooth adapter"),
            Self::NoSuchDevice(m) => write!(f, "no such device {m}"),
            Self::BadState { mac, want, got } => write!(f, "{mac}: cannot {want} while {got}"),
            Self::AdapterOff => write!(f, "adapter is powered off"),
        }
    }
}

impl std::error::Error for BluetoothError {}

#[derive(Debug, Clone)]
pub struct Adapter {
    pub name: String,
    pub address: String,
    pub powered: bool,
    pub discoverable: bool,
    pub discoverable_timeout: u32,
}

impl Default for Adapter {
    fn default() -> Self {
        Self {
            name: "hci0".into(),
            address: "00:00:00:00:00:00".into(),
            powered: true,
            discoverable: false,
            discoverable_timeout: 0,
        }
    }
}

pub struct OmarchyBluetooth {
    pub adapter: Adapter,
    devices: BTreeMap<String, BluetoothDevice>,
}

impl OmarchyBluetooth {
    pub fn new() -> Self {
        Self { adapter: Adapter::default(), devices: BTreeMap::new() }
    }

    // ---- adapter ----
    pub fn power(&mut self, on: bool) {
        self.adapter.powered = on;
        if !on {
            for d in self.devices.values_mut() {
                if d.state == DeviceState::Connected { d.state = DeviceState::Paired; }
            }
        }
    }
    pub fn set_discoverable(&mut self, on: bool, timeout_secs: u32) {
        self.adapter.discoverable = on;
        self.adapter.discoverable_timeout = timeout_secs;
    }

    // ---- discovery ----
    /// Record a device seen during a scan (idempotent; refreshes rssi/state).
    pub fn discover(&mut self, mac: impl Into<String>, name: impl Into<String>, kind: DeviceKind, rssi: i16) -> Result<(), BluetoothError> {
        if !self.adapter.powered { return Err(BluetoothError::AdapterOff); }
        let mac = mac.into();
        if mac.trim().is_empty() { return Err(BluetoothError::EmptyMac); }
        match self.devices.get_mut(&mac) {
            Some(d) => { d.rssi = rssi; d.name = name.into(); }
            None => {
                let mut d = BluetoothDevice::new(mac.clone(), name, kind);
                d.rssi = rssi;
                self.devices.insert(mac, d);
            }
        }
        Ok(())
    }

    // ---- device ops ----
    fn dev_mut(&mut self, mac: &str) -> Result<&mut BluetoothDevice, BluetoothError> {
        self.devices.get_mut(mac).ok_or_else(|| BluetoothError::NoSuchDevice(mac.into()))
    }
    pub fn device(&self, mac: &str) -> Option<&BluetoothDevice> { self.devices.get(mac) }
    pub fn devices(&self) -> impl Iterator<Item = &BluetoothDevice> { self.devices.values() }
    pub fn paired(&self) -> impl Iterator<Item = &BluetoothDevice> {
        self.devices.values().filter(|d| matches!(d.state, DeviceState::Paired | DeviceState::Bonded | DeviceState::Connected | DeviceState::Trusted))
    }

    pub fn pair(&mut self, mac: &str) -> Result<(), BluetoothError> {
        self.require_power()?;
        let d = self.dev_mut(mac)?;
        match d.state {
            DeviceState::Discovered => { d.state = DeviceState::Paired; Ok(()) }
            s if s != DeviceState::Discovered => Ok(()), // already paired+
            _ => unreachable!(),
        }
    }

    pub fn bond(&mut self, mac: &str) -> Result<(), BluetoothError> {
        self.require_power()?;
        let d = self.dev_mut(mac)?;
        if d.state == DeviceState::Discovered {
            return Err(BluetoothError::BadState { mac: mac.into(), want: "bond", got: d.state.label() });
        }
        d.state = DeviceState::Bonded;
        Ok(())
    }

    pub fn connect(&mut self, mac: &str) -> Result<(), BluetoothError> {
        self.require_power()?;
        let d = self.dev_mut(mac)?;
        match d.state {
            DeviceState::Paired | DeviceState::Bonded | DeviceState::Trusted | DeviceState::Connected => {
                d.state = DeviceState::Connected; Ok(())
            }
            s => Err(BluetoothError::BadState { mac: mac.into(), want: "connect", got: s.label() }),
        }
    }

    pub fn disconnect(&mut self, mac: &str) -> Result<(), BluetoothError> {
        let d = self.dev_mut(mac)?;
        if d.state == DeviceState::Connected { d.state = DeviceState::Bonded; Ok(()) }
        else { Err(BluetoothError::BadState { mac: mac.into(), want: "disconnect", got: d.state.label() }) }
    }

    pub fn trust(&mut self, mac: &str, on: bool) -> Result<(), BluetoothError> {
        let d = self.dev_mut(mac)?;
        match (on, d.state) {
            (true, DeviceState::Paired | DeviceState::Bonded | DeviceState::Connected) => {
                d.state = DeviceState::Trusted; Ok(())
            }
            (false, DeviceState::Trusted) => {
                d.state = if d.name.is_empty() { DeviceState::Bonded } else { DeviceState::Bonded }; Ok(())
            }
            (_, s) => Err(BluetoothError::BadState { mac: mac.into(), want: if on { "trust" } else { "untrust" }, got: s.label() }),
        }
    }

    pub fn remove(&mut self, mac: &str) -> Result<BluetoothDevice, BluetoothError> {
        self.devices.remove(mac).ok_or_else(|| BluetoothError::NoSuchDevice(mac.into()))
    }

    fn require_power(&self) -> Result<(), BluetoothError> {
        if self.adapter.powered { Ok(()) } else { Err(BluetoothError::AdapterOff) }
    }

    // ---- reporting ----
    pub fn status_line(&self) -> String {
        let a = &self.adapter;
        format!("Adapter {} ({}) {} discoverable={}", a.name, a.address,
            if a.powered { "powered" } else { "off" }, a.discoverable)
    }
    pub fn device_line(&self, mac: &str) -> Result<String, BluetoothError> {
        let d = self.device(mac).ok_or_else(|| BluetoothError::NoSuchDevice(mac.into()))?;
        Ok(format!("{} {} {} rssi={} services={}", d.mac, d.display_name(), d.state.label(), d.rssi, d.services.len()))
    }
}

impl Default for OmarchyBluetooth {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> OmarchyBluetooth {
        let mut m = OmarchyBluetooth::new();
        m.discover("AA:BB:CC:DD:EE:01", "Pixel Buds", DeviceKind::Le, -55).unwrap();
        m.discover("11:22:33:44:55:66", "Keyboard", DeviceKind::Classic, -70).unwrap();
        m
    }

    #[test]
    fn discover_and_list() {
        let m = mgr();
        assert_eq!(m.devices().count(), 2);
        assert!(m.device("AA:BB:CC:DD:EE:01").is_some());
    }

    #[test]
    fn pair_bond_connect_trust_flow() {
        let mut m = mgr();
        m.pair("AA:BB:CC:DD:EE:01").unwrap();
        assert_eq!(m.device("AA:BB:CC:DD:EE:01").unwrap().state, DeviceState::Paired);
        m.bond("AA:BB:CC:DD:EE:01").unwrap();
        assert_eq!(m.device("AA:BB:CC:DD:EE:01").unwrap().state, DeviceState::Bonded);
        m.connect("AA:BB:CC:DD:EE:01").unwrap();
        assert_eq!(m.device("AA:BB:CC:DD:EE:01").unwrap().state, DeviceState::Connected);
        m.trust("AA:BB:CC:DD:EE:01", true).unwrap();
        assert_eq!(m.device("AA:BB:CC:DD:EE:01").unwrap().state, DeviceState::Trusted);
        assert_eq!(m.paired().count(), 1);
    }

    #[test]
    fn disconnect_returns_to_bonded() {
        let mut m = mgr();
        m.pair("AA:BB:CC:DD:EE:01").unwrap();
        m.connect("AA:BB:CC:DD:EE:01").unwrap();
        m.disconnect("AA:BB:CC:DD:EE:01").unwrap();
        assert_eq!(m.device("AA:BB:CC:DD:EE:01").unwrap().state, DeviceState::Bonded);
    }

    #[test]
    fn bond_requires_paired() {
        let mut m = mgr();
        assert!(m.bond("AA:BB:CC:DD:EE:01").is_err()); // still discovered
        m.pair("AA:BB:CC:DD:EE:01").unwrap();
        assert!(m.bond("AA:BB:CC:DD:EE:01").is_ok());
    }

    #[test]
    fn power_off_blocks_and_disconnects() {
        let mut m = mgr();
        m.pair("AA:BB:CC:DD:EE:01").unwrap();
        m.connect("AA:BB:CC:DD:EE:01").unwrap();
        m.power(false);
        assert_eq!(m.device("AA:BB:CC:DD:EE:01").unwrap().state, DeviceState::Paired);
        assert!(m.pair("11:22:33:44:55:66").is_err()); // AdapterOff
        assert!(m.discover("FF:FF:FF:FF:FF:FF", "x", DeviceKind::Le, -40).is_err());
    }

    #[test]
    fn errors() {
        let mut m = OmarchyBluetooth::new();
        assert!(m.device("XX").is_none());
        assert!(m.pair("NOPE").is_err());
        assert!(m.remove("NOPE").is_err());
        assert!(m.discover("  ", "x", DeviceKind::Le, -40).is_err());
        let mut m2 = OmarchyBluetooth::new();
        m2.discover("M", "a", DeviceKind::Le, -40).unwrap();
        assert!(m2.discover("M", "b", DeviceKind::Le, -40).is_ok()); // refresh, not dup error
    }

    #[test]
    fn remove_and_rediscover() {
        let mut m = mgr();
        m.remove("AA:BB:CC:DD:EE:01").unwrap();
        assert!(m.device("AA:BB:CC:DD:EE:01").is_none());
        m.discover("AA:BB:CC:DD:EE:01", "Back", DeviceKind::Le, -50).unwrap();
        assert_eq!(m.device("AA:BB:CC:DD:EE:01").unwrap().state, DeviceState::Discovered);
    }

    #[test]
    fn status_and_device_line() {
        let mut m = mgr();
        assert!(m.status_line().contains("powered"));
        let s = m.device_line("AA:BB:CC:DD:EE:01").unwrap();
        assert!(s.contains("Pixel Buds") && s.contains("discovered"));
    }
}
