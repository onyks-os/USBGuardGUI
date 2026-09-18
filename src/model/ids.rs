//! Strongly typed identifiers for USBGuard domain objects.
//!
//! [`DeviceId`] and [`RuleId`] wrap the same underlying integer (`u32`) but are
//! distinct types: the compiler rejects any attempt to use one where the other
//! is expected.

/// Unique identifier assigned by the USBGuard daemon to a connected USB device.
///
/// Device IDs are volatile: they are assigned at insertion time and recycled
/// when the device is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId(u32);

impl DeviceId {
    /// Creates a new `DeviceId` from the raw value returned by the daemon.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Positional identifier assigned by the USBGuard daemon to a policy rule.
///
/// Rule IDs are volatile: they shift whenever a rule is added or removed from
/// the policy. Code must never cache a `RuleId` across operations that modify
/// the ruleset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuleId(u32);

impl RuleId {
    /// Creates a new `RuleId` from the raw value returned by the daemon.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceId, RuleId};

    #[test]
    fn device_id_round_trip() {
        let id = DeviceId::new(42);
        assert_eq!(id.get(), 42);
    }

    #[test]
    fn rule_id_round_trip() {
        let id = RuleId::new(7);
        assert_eq!(id.get(), 7);
    }

    #[test]
    fn equality_same_value() {
        assert_eq!(DeviceId::new(1), DeviceId::new(1));
        assert_ne!(DeviceId::new(1), DeviceId::new(2));
    }

    #[test]
    fn debug_format_includes_type_name() {
        let output = format!("{:?}", DeviceId::new(99));
        assert!(output.contains("DeviceId"));
        assert!(output.contains("99"));
    }
}
