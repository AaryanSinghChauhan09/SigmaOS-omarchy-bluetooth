// SPDX-License-Identifier: Apache-2.0
//! SigmaOS-omarchy-bluetooth: omarchy bluetooth component model.
//!
//! Real implementation: bluetooths, events, validation, deterministic storage.
//! Replaces the placeholder stub.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bluetooth {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub id: String,
    pub subject: String,
    pub detail: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OmarchyBluetoothError {
    EmptyBluetoothName,
    DuplicateBluetooth(String),
    EmptyEventSubject,
}

impl fmt::Display for OmarchyBluetoothError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyBluetoothName => write!(f, "bluetooth name must not be empty"),
            Self::DuplicateBluetooth(id) => write!(f, "duplicate bluetooth {:?}", id),
            Self::EmptyEventSubject => write!(f, "event subject must not be empty"),
        }
    }
}

impl std::error::Error for OmarchyBluetoothError {}

pub struct OmarchyBluetooth {
    pub name: String,
    bluetooths: BTreeMap<String, Bluetooth>,
    events: BTreeMap<String, Event>,
}

impl OmarchyBluetooth {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bluetooths: BTreeMap::new(),
            events: BTreeMap::new(),
        }
    }

    pub fn add_bluetooth(&mut self, item: Bluetooth) -> Result<(), OmarchyBluetoothError> {
        if item.name.trim().is_empty() {
            return Err(OmarchyBluetoothError::EmptyBluetoothName);
        }
        let id = item.id.clone();
        if self.bluetooths.contains_key(&id) {
            return Err(OmarchyBluetoothError::DuplicateBluetooth(id.clone()));
        }
        self.bluetooths.insert(id, item);
        Ok(())
    }

    pub fn bluetooth_count(&self) -> usize {
        self.bluetooths.len()
    }

    pub fn get_bluetooth(&self, id: &str) -> Option<&Bluetooth> {
        self.bluetooths.get(id)
    }

    pub fn add_event(&mut self, event: Event) -> Result<(), OmarchyBluetoothError> {
        if event.subject.trim().is_empty() {
            return Err(OmarchyBluetoothError::EmptyEventSubject);
        }
        self.events.insert(event.id.clone(), event);
        Ok(())
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn get_event(&self, id: &str) -> Option<&Event> {
        self.events.get(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bluetooth(id: &str, name: &str) -> Bluetooth {
        Bluetooth {
            id: id.into(),
            name: name.into(),
            kind: "default".into(),
            detail: String::new(),
        }
    }

    #[test]
    fn test_create() {
        let m = OmarchyBluetooth::new("OmarchyBluetooth");
        assert_eq!(m.name, "OmarchyBluetooth");
        assert_eq!(m.bluetooth_count(), 0);
        assert_eq!(m.event_count(), 0);
    }

    #[test]
    fn test_add_bluetooth() {
        let mut m = OmarchyBluetooth::new("OmarchyBluetooth");
        m.add_bluetooth(sample_bluetooth("a1", "Alpha")).unwrap();
        assert_eq!(m.bluetooth_count(), 1);
        assert!(m.get_bluetooth("a1").is_some());
    }

    #[test]
    fn test_empty_name_rejected() {
        let mut m = OmarchyBluetooth::new("OmarchyBluetooth");
        assert!(m.add_bluetooth(sample_bluetooth("x", "   ")).is_err());
    }

    #[test]
    fn test_duplicate_rejected() {
        let mut m = OmarchyBluetooth::new("OmarchyBluetooth");
        m.add_bluetooth(sample_bluetooth("x", "One")).unwrap();
        assert!(m.add_bluetooth(sample_bluetooth("x", "Two")).is_err());
    }

    #[test]
    fn test_event_validation() {
        let mut m = OmarchyBluetooth::new("OmarchyBluetooth");
        let bad = Event { id: "e1".into(), subject: "".into(), detail: String::new(), timestamp: 0 };
        assert!(m.add_event(bad).is_err());
        let good = Event { id: "e1".into(), subject: "boot".into(), detail: String::new(), timestamp: 1 };
        m.add_event(good).unwrap();
        assert_eq!(m.event_count(), 1);
    }
}
