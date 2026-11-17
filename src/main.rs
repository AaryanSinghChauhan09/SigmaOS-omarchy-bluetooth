//! SigmaOS-omarchy-bluetooth — bluetoothctl-style CLI.
//!
//! Usage:
//!   omarchy-bluetooth status
//!   omarchy-bluetooth power on|off
//!   omarchy-bluetooth discoverable [on|off] [timeout]
//!   omarchy-bluetooth scan            (seeded demo devices; BlueZ feeds live)
//!   omarchy-bluetooth devices [--paired]
//!   omarchy-bluetooth info <mac>
//!   omarchy-bluetooth pair|bond|connect|disconnect|trust|untrust|remove <mac>

use omarchy_bluetooth::{DeviceKind, OmarchyBluetooth};
use std::process::exit;

fn usage() -> ! {
    eprintln!("usage: omarchy-bluetooth <status|power|discoverable|scan|devices|info|pair|bond|connect|disconnect|trust|untrust|remove> [args]");
    exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() { usage(); }
    let state = OmarchyBluetooth::state_path();
    // load persisted paired/bonded devices, then merge fresh demo discoveries
    // (like BlueZ: paired devices persist, discoveries are transient)
    let mut bt = OmarchyBluetooth::load_from(&state).unwrap_or_else(|_| OmarchyBluetooth::new());
    for (mac, name, kind, rssi) in [
        ("AA:BB:CC:DD:EE:01", "Pixel Buds Pro", DeviceKind::Le, -52),
        ("11:22:33:44:55:66", "MX Keys", DeviceKind::Classic, -68),
        ("22:33:44:55:66:77", "Sony WH-1000XM5", DeviceKind::Dual, -60),
    ] {
        if bt.device(mac).is_none() {
            let _ = bt.discover(mac, name, kind, rssi);
        }
    }
    let cmd = args[0].as_str();
    let rest = &args[1..];
    let mac = |i: usize| -> &str { rest.get(i).map(|s| s.as_str()).unwrap_or_else(|| usage()) };

    let result: Result<(), String> = match cmd {
        "status" => { println!("{}", bt.status_line()); Ok(()) }
        "power" => {
            let on = match rest.first().map(|s| s.as_str()) {
                Some("on") | Some("1") | Some("true") => true,
                Some("off") | Some("0") | Some("false") => false,
                _ => usage(),
            };
            bt.power(on); println!("{}", bt.status_line()); Ok(())
        }
        "discoverable" => {
            let on = match rest.first().map(|s| s.as_str()) {
                Some("off") | Some("0") => false,
                _ => true,
            };
            let t: u32 = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            bt.set_discoverable(on, t); println!("discoverable={on} timeout={t}"); Ok(())
        }
        "scan" => {
            // In production BlueZ pushes discoveries; here we re-seed and report.
            for d in bt.devices() { println!("{}", bt.device_line(&d.mac.clone()).unwrap_or_default()); }
            Ok(())
        }
        "devices" => {
            let paired_only = rest.iter().any(|s| s == "--paired" || s == "-p");
            if paired_only {
                for d in bt.paired() { println!("{}", bt.device_line(&d.mac.clone()).unwrap_or_default()); }
            } else {
                for d in bt.devices() { println!("{}", bt.device_line(&d.mac.clone()).unwrap_or_default()); }
            }
            Ok(())
        }
        "info" => {
            let m = mac(0);
            match bt.device_line(m) {
                Ok(l) => { println!("{l}");
                    if let Some(d) = bt.device(m) {
                        println!("  kind={} rssi={} services={:?}", d.kind, d.rssi, d.services);
                    }
                    Ok(())
                }
                Err(e) => Err(e.to_string()),
            }
        }
        "pair" | "bond" | "connect" | "disconnect" => {
            let m = mac(0);
            let r = match cmd {
                "pair" => bt.pair(m),
                "bond" => bt.bond(m),
                "connect" => bt.connect(m),
                _ => bt.disconnect(m),
            };
            r.map(|_| println!("ok")).map_err(|e| e.to_string())
        }
        "trust" | "untrust" => {
            let m = mac(0);
            bt.trust(m, cmd == "trust").map(|_| println!("ok")).map_err(|e| e.to_string())
        }
        "remove" => {
            let m = mac(0);
            bt.remove(m).map(|_| println!("removed")).map_err(|e| e.to_string())
        }
        _ => usage(),
    };

    match result {
        Ok(()) => {
            if let Err(e) = bt.save_to(&state) { eprintln!("omarchy-bluetooth: warn: save: {e}"); }
        }
        Err(e) => { eprintln!("omarchy-bluetooth: {e}"); exit(1); }
    }
}
