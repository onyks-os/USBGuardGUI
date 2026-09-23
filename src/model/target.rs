//! Device authorization targets, device events, and the persistence choice.
//!
//! Every numeric value the daemon sends is decoded through a *total* function
//! with a catch-all arm (docs/architecture.md §2.3): an unexpected number
//! becomes "unknown", never a confident wrong label.

use std::fmt;

/// The authorization state of a device, decoded from the daemon's `target` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// The device is authorized.
    Allow,
    /// The device is present but not authorized.
    Block,
    /// The device was removed from the system.
    Reject,
    /// Any value outside the three device states, kept verbatim.
    Other(u32),
}

impl From<u32> for Target {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Allow,
            1 => Self::Block,
            2 => Self::Reject,
            n => Self::Other(n),
        }
    }
}

impl From<DevicePolicy> for Target {
    fn from(p: DevicePolicy) -> Self {
        match p {
            DevicePolicy::Allow => Self::Allow,
            DevicePolicy::Block => Self::Block,
            DevicePolicy::Reject => Self::Reject,
        }
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Allow => f.write_str("allowed"),
            Self::Block => f.write_str("blocked"),
            Self::Reject => f.write_str("rejected"),
            Self::Other(n) => write!(f, "unknown ({n})"),
        }
    }
}

/// A target that can be *sent* to the daemon with `applyDevicePolicy`.
///
/// There is deliberately no conversion from [`Target::Other`]: an unknown
/// state can never be echoed back to the daemon as a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DevicePolicy {
    /// Authorize the device.
    Allow,
    /// Deauthorize the device, leaving it connected.
    Block,
    /// Remove the device from the system; undoing it needs re-insertion.
    Reject,
}

impl DevicePolicy {
    /// The wire encoding used by `Devices1.applyDevicePolicy`.
    #[must_use]
    pub const fn wire(self) -> u32 {
        match self {
            Self::Allow => 0,
            Self::Block => 1,
            Self::Reject => 2,
        }
    }
}

impl TryFrom<Target> for DevicePolicy {
    type Error = Target;

    fn try_from(t: Target) -> Result<Self, Self::Error> {
        match t {
            Target::Allow => Ok(Self::Allow),
            Target::Block => Ok(Self::Block),
            Target::Reject => Ok(Self::Reject),
            other @ Target::Other(_) => Err(other),
        }
    }
}

/// The `event` field of `DevicePresenceChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceEvent {
    /// The device was already present when the daemon started.
    Present,
    /// The device was just inserted.
    Insert,
    /// The device's attributes changed.
    Update,
    /// The device was removed.
    Remove,
    /// A value outside the known set, kept verbatim.
    Other(u32),
}

impl From<u32> for DeviceEvent {
    fn from(v: u32) -> Self {
        match v {
            0 => Self::Present,
            1 => Self::Insert,
            2 => Self::Update,
            3 => Self::Remove,
            n => Self::Other(n),
        }
    }
}

/// Whether a policy change survives a daemon restart.
///
/// The daemon expresses this with two booleans of *opposite* polarity
/// (docs/architecture.md §2.4.7); the two accessors below are the only places
/// in the program where those booleans are produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Persistence {
    /// Written to the ruleset; survives a daemon restart.
    Permanent,
    /// In-memory only; lost on daemon restart. The default, because the less
    /// destructive choice is the one whose effect disappears.
    #[default]
    RuntimeOnly,
}

impl Persistence {
    /// For `Policy1.appendRule`, whose parameter is `temporary`.
    #[must_use]
    pub const fn as_temporary(self) -> bool {
        matches!(self, Self::RuntimeOnly)
    }

    /// For `Devices1.applyDevicePolicy`, whose parameter is `permanent`.
    #[must_use]
    pub const fn as_permanent(self) -> bool {
        matches!(self, Self::Permanent)
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceEvent, DevicePolicy, Persistence, Target};

    #[test]
    fn target_decoding_is_total() {
        assert_eq!(Target::from(0), Target::Allow);
        assert_eq!(Target::from(1), Target::Block);
        assert_eq!(Target::from(2), Target::Reject);
        assert_eq!(Target::from(3), Target::Other(3));
        assert_eq!(Target::from(u32::MAX), Target::Other(u32::MAX));
    }

    #[test]
    fn unknown_target_cannot_become_a_command() {
        assert!(DevicePolicy::try_from(Target::Other(5)).is_err());
        for p in [
            DevicePolicy::Allow,
            DevicePolicy::Block,
            DevicePolicy::Reject,
        ] {
            assert_eq!(DevicePolicy::try_from(Target::from(p)), Ok(p));
            assert_eq!(Target::from(p.wire()), Target::from(p));
        }
    }

    #[test]
    fn event_decoding_is_total() {
        assert_eq!(DeviceEvent::from(3), DeviceEvent::Remove);
        assert_eq!(DeviceEvent::from(9), DeviceEvent::Other(9));
    }

    #[test]
    fn persistence_booleans_are_inverse() {
        for p in [Persistence::Permanent, Persistence::RuntimeOnly] {
            assert_ne!(p.as_temporary(), p.as_permanent());
        }
        assert!(Persistence::Permanent.as_permanent());
        assert!(Persistence::RuntimeOnly.as_temporary());
        assert_eq!(Persistence::default(), Persistence::RuntimeOnly);
    }
}
