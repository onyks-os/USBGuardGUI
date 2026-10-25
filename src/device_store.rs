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
#[path = "../tests/unit/device_store.rs"]
mod tests;
