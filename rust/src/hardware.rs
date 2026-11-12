//! USB connection history and exclusive access to physical serial devices.
use crate::boards::{self, PortIdentity};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// Stable across macOS tty/cu aliases and USB port renumbering when a serial is present.
pub(crate) fn device_key(port: &PortIdentity) -> String {
    if let Some(serial) = port
        .serial_number
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        format!(
            "usb:{}:{}",
            port.vendor_id.as_deref().unwrap_or("").to_ascii_lowercase(),
            serial.trim()
        )
    } else {
        port.path.replace("/dev/tty.", "/dev/cu.")
    }
}

/// macOS enumerates both callout and dial-in aliases. Keep one entry per
/// physical device and prefer the callout path for development tools.
pub(crate) fn canonical_ports(ports: Vec<PortIdentity>) -> Vec<PortIdentity> {
    let mut unique = std::collections::BTreeMap::<String, PortIdentity>::new();
    for port in ports {
        let key = device_key(&port);
        let candidate = (port.path.starts_with("/dev/cu."), &port.path);
        if unique
            .get(&key)
            .is_none_or(|old| candidate > (old.path.starts_with("/dev/cu."), &old.path))
        {
            unique.insert(key, port);
        }
    }
    let mut ports: Vec<_> = unique.into_values().collect();
    ports.sort_by(|left, right| left.path.cmp(&right.path));
    ports
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentBoard {
    pub(crate) key: String,
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) board_id: Option<String>,
    pub(crate) vendor_id: Option<String>,
    pub(crate) product_id: Option<String>,
    pub(crate) serial_number: Option<String>,
    pub(crate) last_connected: u64,
}

impl RecentBoard {
    pub(crate) fn from_port(
        port: &PortIdentity,
        overrides: &HashMap<String, String>,
        now: u64,
    ) -> Self {
        let resolved = boards::identify_board(Some(port), overrides);
        Self {
            key: device_key(port),
            path: port.path.clone(),
            name: resolved
                .board
                .map(|b| b.name.to_string())
                .unwrap_or_else(|| {
                    port.manufacturer
                        .as_ref()
                        .map(|m| format!("{m} USB board"))
                        .unwrap_or_else(|| "USB serial board".into())
                }),
            board_id: resolved.board.map(|b| b.id.to_string()),
            vendor_id: port.vendor_id.clone(),
            product_id: port.product_id.clone(),
            serial_number: port.serial_number.clone(),
            last_connected: now,
        }
    }
}

/// Update history only for attach/reconnect, descriptor changes or explicit overrides.
/// A polling tick never rewrites last-connected times for devices that stayed present.
pub(crate) fn remember_connections(
    history: &mut Vec<RecentBoard>,
    previous: &[PortIdentity],
    current: &[PortIdentity],
    overrides: &HashMap<String, String>,
    now: u64,
) -> bool {
    let previous_keys: HashSet<_> = previous.iter().map(device_key).collect();
    let mut changed = false;
    for port in current
        .iter()
        .filter(|port| port.vendor_id.is_some() || crate::profiles::is_likely_hardware_port(port))
    {
        let next = RecentBoard::from_port(port, overrides, now);
        let existing = history.iter().position(|board| board.key == next.key);
        let update = existing.is_none_or(|ix| {
            let old = &history[ix];
            !previous_keys.contains(&next.key)
                || old.path != next.path
                || old.board_id != next.board_id
                || old.name != next.name
        });
        if update {
            if let Some(ix) = existing {
                history.remove(ix);
            }
            history.insert(0, next);
            changed = true;
        }
    }
    history.truncate(12);
    changed
}

/// Shared by workspace commands, REPL and all flash panels.
#[derive(Clone, Default)]
pub(crate) struct PortLeases(Arc<Mutex<HashSet<String>>>);

pub(crate) struct PortLease {
    registry: PortLeases,
    key: String,
}

impl PortLeases {
    pub(crate) fn acquire(&self, key: &str) -> Result<PortLease, String> {
        let mut ports = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if !ports.insert(key.to_string()) {
            return Err("This board is in use by another panel or the REPL. Finish that operation or disconnect the REPL first.".into());
        }
        Ok(PortLease {
            registry: self.clone(),
            key: key.to_string(),
        })
    }
    pub(crate) fn is_busy(&self, key: &str) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(key)
    }
}
impl Drop for PortLease {
    fn drop(&mut self) {
        self.registry
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn port(path: &str, serial: &str) -> PortIdentity {
        PortIdentity {
            path: path.into(),
            serial_number: Some(serial.into()),
            vendor_id: Some("303a".into()),
            product_id: Some("1001".into()),
            ..Default::default()
        }
    }
    #[test]
    fn usb_aliases_are_one_device_and_do_not_churn_history() {
        let snapshots = vec![
            port("/dev/tty.usb1", "one"),
            port("/dev/cu.usb1", "one"),
            port("/dev/cu.usb2", "two"),
        ];
        let ports = canonical_ports(snapshots);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].path, "/dev/cu.usb1");
        let mut history = Vec::new();
        assert!(remember_connections(
            &mut history,
            &[],
            &ports,
            &HashMap::new(),
            10
        ));
        assert!(!remember_connections(
            &mut history,
            &ports,
            &ports,
            &HashMap::new(),
            20
        ));
        assert_eq!(history.len(), 2);
        let mut callout = port("/dev/cu.unknown", "");
        callout.serial_number = None;
        let mut dialin = callout.clone();
        dialin.path = "/dev/tty.unknown".into();
        assert_eq!(canonical_ports(vec![callout, dialin]).len(), 1);
    }

    #[test]
    fn reconnect_renumbering_updates_one_history_entry_without_poll_churn() {
        let mut history = Vec::new();
        let first = vec![port("/dev/cu.usb1", "one")];
        assert!(remember_connections(
            &mut history,
            &[],
            &first,
            &HashMap::new(),
            10
        ));
        assert!(!remember_connections(
            &mut history,
            &first,
            &first,
            &HashMap::new(),
            20
        ));
        assert_eq!(history[0].last_connected, 10);
        assert!(!remember_connections(
            &mut history,
            &first,
            &[],
            &HashMap::new(),
            30
        ));
        let reconnected = vec![port("/dev/cu.usb2", "one")];
        assert!(remember_connections(
            &mut history,
            &[],
            &reconnected,
            &HashMap::new(),
            40
        ));
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].path, "/dev/cu.usb2");
        assert_eq!(history[0].last_connected, 40);
        assert_eq!(history[0].board_id.as_deref(), Some("esp32-s3-generic"));
    }
    #[test]
    fn history_preserves_disconnected_boards_and_caps_recent_devices() {
        let mut history = Vec::new();
        for ix in 0..15 {
            remember_connections(
                &mut history,
                &[],
                &[port(&format!("/dev/cu.usb{ix}"), &ix.to_string())],
                &HashMap::new(),
                ix,
            );
        }
        assert_eq!(history.len(), 12);
        assert_eq!(history[0].serial_number.as_deref(), Some("14"));
        assert!(!remember_connections(
            &mut history,
            &[],
            &[],
            &HashMap::new(),
            20
        ));
        assert_eq!(history.len(), 12);
    }
    #[test]
    fn port_exclusivity_is_per_physical_board_and_released_on_drop() {
        let registry = PortLeases::default();
        let first = port("/dev/cu.one", "one");
        let alias = port("/dev/tty.one", "one");
        let lease = registry.acquire(&device_key(&first)).unwrap();
        assert!(registry.acquire(&device_key(&alias)).is_err());
        let other = registry
            .acquire(&device_key(&port("/dev/cu.two", "two")))
            .unwrap();
        drop(lease);
        assert!(registry.acquire(&device_key(&alias)).is_ok());
        drop(other);
        assert!(!registry.is_busy(&device_key(&port("/dev/cu.two", "two"))));
    }
}
