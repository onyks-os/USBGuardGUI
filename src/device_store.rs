//! The interface's device table as pure data: applying snapshots and
//! coalesced batches, and the presentation order (docs/architecture.md §9.2).
//!
//! Kept out of `ui` so it is tested without GTK or a display.

use std::collections::BTreeMap;

use crate::model::{Device, DeviceAttributes, DeviceDelta, DeviceId, PendingKind, Target};
use crate::rules::parse_device;

/// Every device currently known, by daemon id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceStore {
    devices: BTreeMap<DeviceId, Device>,
}

impl DeviceStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces everything with a fresh `listDevices` result. An operation in
    /// flight on a device that is still present stays marked.
    pub fn replace(&mut self, devices: Vec<Device>) {
        let old = std::mem::take(&mut self.devices);
        self.devices = devices
            .into_iter()
            .map(|mut d| {
                d.pending = old.get(&d.id).and_then(|o| o.pending);
                (d.id, d)
            })
            .collect();
    }

    /// Marks or clears an operation in flight on a device. No-op if the device
    /// is gone.
    pub fn set_pending(&mut self, id: DeviceId, pending: Option<PendingKind>) {
        if let Some(d) = self.devices.get_mut(&id) {
            d.pending = pending;
        }
    }

    /// Drops everything: nothing observed before a disconnection is trusted
    /// after it (§5.5).
    pub fn clear(&mut self) {
        self.devices.clear();
    }

    /// Applies one coalescing window's worth of changes.
    pub fn apply(&mut self, batch: Vec<DeviceDelta>) {
        for delta in batch {
            if delta.removed {
                self.devices.remove(&delta.id);
                continue;
            }
            let device = self.devices.entry(delta.id).or_insert_with(|| Device {
                id: delta.id,
                target: Target::Other(u32::MAX),
                rule_text: String::new(),
                attrs: DeviceAttributes::default(),
                parse_error: None,
                pending: None,
            });
            if let Some(text) = delta.rule_text.filter(|t| !t.is_empty()) {
                // The rule text is the fullest description; re-parse it, then
                // let the signal's own fields win.
                let reparsed = parse_device(delta.id, text);
                device.rule_text = reparsed.rule_text;
                device.attrs = reparsed.attrs;
                device.parse_error = reparsed.parse_error;
                device.target = reparsed.target;
            }
            device.attrs.merge_from(delta.attrs);
            if let Some(target) = delta.target {
                device.target = target;
            }
        }
    }

    /// Number of devices.
    #[must_use]
    pub fn len(&self) -> usize {
        self.devices.len()
    }

    /// True when no device is known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Devices in presentation order: by port, which groups them by physical
    /// topology and is stable, unlike `listDevices` order (§2.4.9).
    #[must_use]
    pub fn sorted(&self) -> Vec<Device> {
        let mut list: Vec<Device> = self.devices.values().cloned().collect();
        list.sort_by(|a, b| {
            port_key(a.attrs.via_port.as_deref())
                .cmp(&port_key(b.attrs.via_port.as_deref()))
                .then(a.id.cmp(&b.id))
        });
        list
    }
}

/// Sort key for a port: `usbN` root hubs first, then `bus-port.port…`
/// numerically, unknown ports last.
#[must_use]
pub fn port_key(port: Option<&str>) -> (u8, Vec<u32>) {
    match port.map(str::trim).filter(|p| !p.is_empty()) {
        None => (2, Vec::new()),
        Some(p) => {
            let root = p.strip_prefix("usb");
            let parts = root
                .unwrap_or(p)
                .split(['-', '.'])
                .map(|s| s.parse().unwrap_or(u32::MAX))
                .collect();
            (u8::from(root.is_none()), parts)
        }
    }
}

/// Whether a device matches the filter entry: case-insensitive over name,
/// `vendor:product`, and serial (§9.2).
#[must_use]
pub fn matches_filter(device: &Device, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    let hit = |field: Option<String>| field.is_some_and(|f| f.to_lowercase().contains(&query));
    hit(device.attrs.name.clone())
        || hit(device.attrs.serial.clone())
        || hit(device.attrs.usb_id.map(|id| id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{DeviceStore, matches_filter, port_key};
    use crate::model::{DeviceAttributes, DeviceDelta, DeviceId, PendingKind, Target};
    use crate::rules::parse_device;

    fn delta(id: u32) -> DeviceDelta {
        DeviceDelta {
            id: DeviceId::new(id),
            removed: false,
            target: None,
            rule_text: None,
            attrs: DeviceAttributes::default(),
        }
    }

    #[test]
    fn batches_insert_update_and_remove() {
        let mut store = DeviceStore::new();
        store.replace(vec![parse_device(
            DeviceId::new(1),
            r#"allow id 1111:0001 name "Keyboard" via-port "1-1""#.into(),
        )]);

        let mut inserted = delta(2);
        inserted.rule_text = Some(r#"block id 2222:0001 name "Disk" via-port "1-2""#.into());
        let mut changed = delta(1);
        changed.target = Some(Target::Block);
        store.apply(vec![changed, inserted]);

        let list = store.sorted();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].target, Target::Block);
        assert_eq!(list[0].attrs.name.as_deref(), Some("Keyboard"));
        assert_eq!(list[1].attrs.name.as_deref(), Some("Disk"));

        let mut removed = delta(1);
        removed.removed = true;
        store.apply(vec![removed]);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn pending_marks_survive_a_snapshot() {
        let mut store = DeviceStore::new();
        let device = parse_device(DeviceId::new(1), r#"block name "X""#.into());
        store.replace(vec![device.clone()]);
        store.set_pending(DeviceId::new(1), Some(PendingKind::Started));
        store.replace(vec![device]);
        assert_eq!(store.sorted()[0].pending, Some(PendingKind::Started));
    }

    #[test]
    fn a_policy_target_in_the_delta_beats_the_rule_text() {
        let mut store = DeviceStore::new();
        let mut d = delta(5);
        d.rule_text = Some(r#"block name "X""#.into());
        d.target = Some(Target::Allow);
        store.apply(vec![d]);
        assert_eq!(store.sorted()[0].target, Target::Allow);
    }

    #[test]
    fn ordering_and_filtering() {
        assert!(port_key(Some("usb2")) < port_key(Some("1-1")));
        assert!(port_key(Some("1-2")) < port_key(Some("1-10")));
        assert!(port_key(Some("1-2")) < port_key(None));
        let d = parse_device(
            DeviceId::new(1),
            r#"allow id 1111:0001 name "Example Keyboard" serial "AB12""#.into(),
        );
        assert!(matches_filter(&d, "keyb"));
        assert!(matches_filter(&d, "1111:"));
        assert!(matches_filter(&d, "ab12"));
        assert!(matches_filter(&d, "  "));
        assert!(!matches_filter(&d, "mouse"));
    }
}
