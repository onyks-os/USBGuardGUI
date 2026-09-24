//! Devices as the interface knows them (docs/architecture.md §6.1–§6.2).

use std::collections::HashMap;

use super::error::ParseError;
use super::ids::{DeviceId, InterfaceType, UsbId};
use super::rule::{AttributeName, AttributeValue, Rule, RuleTarget};
use super::target::Target;

/// Every field optional: a missing attribute is normal, not an error (§6.2).
///
/// Strings here are already converted for display (lossily, if the device
/// supplied bytes that are not UTF-8).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeviceAttributes {
    /// The product string.
    pub name: Option<String>,
    /// `vendor:product`.
    pub usb_id: Option<UsbId>,
    /// The serial number string.
    pub serial: Option<String>,
    /// The port, e.g. `3-2` or `usb1`.
    pub via_port: Option<String>,
    /// Hash of the device descriptor.
    pub hash: Option<String>,
    /// Hash of the parent device.
    pub parent_hash: Option<String>,
    /// Interface types, in the order reported.
    pub with_interface: Vec<InterfaceType>,
    /// Connection type, e.g. `hotplug`.
    pub with_connect_type: Option<String>,
}

impl DeviceAttributes {
    /// Extracts the attributes of a parsed device rule.
    #[must_use]
    pub fn from_rule(rule: &Rule) -> Self {
        let text = |name| {
            rule.string_value(name)
                .map(|s| s.to_string_lossy().into_owned())
        };
        let usb_id = rule.attribute(AttributeName::Id).and_then(|a| {
            a.values.as_slice().iter().find_map(|v| match v {
                AttributeValue::UsbId(id) => Some(*id),
                _ => None,
            })
        });
        let with_interface = rule
            .attribute(AttributeName::WithInterface)
            .map(|a| {
                a.values
                    .as_slice()
                    .iter()
                    .filter_map(|v| match v {
                        AttributeValue::Interface(i) => Some(*i),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            name: text(AttributeName::Name),
            usb_id,
            serial: text(AttributeName::Serial),
            via_port: text(AttributeName::ViaPort),
            hash: text(AttributeName::Hash),
            parent_hash: text(AttributeName::ParentHash),
            with_interface,
            with_connect_type: text(AttributeName::WithConnectType),
        }
    }

    /// Builds attributes from the `a{ss}` dictionary carried by the device
    /// signals. Unknown keys are ignored; malformed values are dropped, never
    /// fatal.
    #[must_use]
    pub fn from_signal_map(map: &HashMap<String, String>) -> Self {
        let get = |key: &str| map.get(key).cloned();
        Self {
            name: get("name"),
            usb_id: map.get("id").and_then(|v| UsbId::parse(v.as_bytes())),
            serial: get("serial"),
            via_port: get("via-port"),
            hash: get("hash"),
            parent_hash: get("parent-hash"),
            with_interface: map
                .get("with-interface")
                .map(|v| parse_interface_list(v))
                .unwrap_or_default(),
            with_connect_type: get("with-connect-type"),
        }
    }

    /// Overlays `newer` on `self`: every field `newer` has wins (§5.4 rule 3).
    pub fn merge_from(&mut self, newer: Self) {
        fn take<T>(slot: &mut Option<T>, newer: Option<T>) {
            if newer.is_some() {
                *slot = newer;
            }
        }
        take(&mut self.name, newer.name);
        take(&mut self.usb_id, newer.usb_id);
        take(&mut self.serial, newer.serial);
        take(&mut self.via_port, newer.via_port);
        take(&mut self.hash, newer.hash);
        take(&mut self.parent_hash, newer.parent_hash);
        take(&mut self.with_connect_type, newer.with_connect_type);
        if !newer.with_interface.is_empty() {
            self.with_interface = newer.with_interface;
        }
    }
}

/// Parses the signal form of `with-interface`: either `cc:ss:pp` or a braced
/// list `{ cc:ss:pp cc:ss:pp }`. Invalid entries are skipped.
fn parse_interface_list(text: &str) -> Vec<InterfaceType> {
    text.split(|c: char| c.is_whitespace() || c == '{' || c == '}')
        .filter(|s| !s.is_empty())
        .filter_map(|s| InterfaceType::parse(s.as_bytes()))
        .collect()
}

/// A mutating operation in flight on a device, and what the row should say
/// about it (docs/architecture.md §5.7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PendingKind {
    /// Just started: spinner and a Cancel button.
    Started,
    /// Running for 20 s: "Waiting for authentication".
    WaitingForAuthentication,
    /// Running for 180 s: a warning that no Polkit agent may be running. The
    /// operation is **not** cancelled.
    PossiblyNoAgent,
}

/// One device as the interface knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// The daemon's table index. Not the `id` attribute.
    pub id: DeviceId,
    /// Authorization state.
    pub target: Target,
    /// Rule text as returned by `listDevices`, verbatim.
    pub rule_text: String,
    /// Parsed attributes; empty when the text could not be parsed.
    pub attrs: DeviceAttributes,
    /// Set when the rule text could not be parsed. The row is still shown,
    /// with the raw text: hiding a device is the worst possible failure (§7.2).
    pub parse_error: Option<ParseError>,
    /// Set when a mutating operation on this device is in flight.
    pub pending: Option<PendingKind>,
}

impl Device {
    /// Builds a device from a `listDevices` entry and the result of parsing
    /// its text. Never fails: an unparseable rule still produces a row.
    ///
    /// The parsing itself is done by the caller (see
    /// [`crate::rules::parse_device`]) so that `model` depends on nothing.
    #[must_use]
    pub fn from_parsed(id: DeviceId, rule_text: String, parsed: Result<&Rule, ParseError>) -> Self {
        let (target, attrs, parse_error) = match parsed {
            Ok(rule) => (
                rule.target
                    .map_or(Target::Other(u32::MAX), rule_target_to_target),
                DeviceAttributes::from_rule(rule),
                None,
            ),
            Err(err) => (
                leading_target(&rule_text),
                DeviceAttributes::default(),
                Some(err),
            ),
        };
        Self {
            id,
            target,
            rule_text,
            attrs,
            parse_error,
            pending: None,
        }
    }
}

const fn rule_target_to_target(t: RuleTarget) -> Target {
    match t {
        RuleTarget::Allow => Target::Allow,
        RuleTarget::Block => Target::Block,
        RuleTarget::Reject => Target::Reject,
        RuleTarget::Match => Target::Other(u32::MAX),
    }
}

/// Best-effort target from the first word, for text the parser rejected.
fn leading_target(text: &str) -> Target {
    text.split_whitespace()
        .next()
        .and_then(|w| RuleTarget::from_keyword(w.as_bytes()))
        .map_or(Target::Other(u32::MAX), rule_target_to_target)
}

/// The aggregate produced by one coalescing window for one device (§5.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceDelta {
    /// Which device.
    pub id: DeviceId,
    /// The device left during the window: drop its row.
    pub removed: bool,
    /// The latest known target, if any event carried one.
    pub target: Option<Target>,
    /// The latest rule text, if any event carried one.
    pub rule_text: Option<String>,
    /// Accumulated attributes; later keys win.
    pub attrs: DeviceAttributes,
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::DeviceAttributes;

    #[test]
    fn signal_map_is_forgiving() {
        let mut m = HashMap::new();
        m.insert("id".into(), "not-an-id".into());
        m.insert("name".into(), "Keyboard".into());
        m.insert(
            "with-interface".into(),
            "{ 03:01:01 bogus 03:00:00 }".into(),
        );
        m.insert("unexpected".into(), "ignored".into());
        let a = DeviceAttributes::from_signal_map(&m);
        assert_eq!(a.name.as_deref(), Some("Keyboard"));
        assert!(a.usb_id.is_none());
        assert_eq!(a.with_interface.len(), 2);
        assert!(a.serial.is_none());
    }

    #[test]
    fn merge_keeps_old_values_newer_wins() {
        let mut a = DeviceAttributes {
            name: Some("old".into()),
            serial: Some("S".into()),
            ..DeviceAttributes::default()
        };
        a.merge_from(DeviceAttributes {
            name: Some("new".into()),
            ..DeviceAttributes::default()
        });
        assert_eq!(a.name.as_deref(), Some("new"));
        assert_eq!(a.serial.as_deref(), Some("S"));
    }
}
