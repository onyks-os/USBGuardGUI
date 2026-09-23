//! The two runtime parameters reachable over D-Bus (docs/architecture.md §2.5).
//!
//! The daemon accepts only a closed set of names, so the set is an enum, not a
//! string. Every other `usbguard-daemon.conf` setting is startup configuration
//! and cannot be changed over D-Bus at all.

use std::fmt;

/// A runtime parameter name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Parameter {
    /// Treatment of a device matching no rule: `allow`, `block`, `reject`.
    ImplicitPolicyTarget,
    /// Treatment of a device inserted while the daemon runs:
    /// `block`, `reject`, `apply-policy`.
    InsertedDevicePolicy,
}

impl Parameter {
    /// Both parameters.
    pub const ALL: [Self; 2] = [Self::ImplicitPolicyTarget, Self::InsertedDevicePolicy];

    /// The name as the daemon knows it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ImplicitPolicyTarget => "ImplicitPolicyTarget",
            Self::InsertedDevicePolicy => "InsertedDevicePolicy",
        }
    }

    /// The values the daemon accepts for this parameter.
    #[must_use]
    pub const fn allowed_values(self) -> &'static [&'static str] {
        match self {
            Self::ImplicitPolicyTarget => &["allow", "block", "reject"],
            Self::InsertedDevicePolicy => &["block", "reject", "apply-policy"],
        }
    }

    /// The inverse of [`Parameter::name`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

impl fmt::Display for Parameter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::Parameter;

    #[test]
    fn names_round_trip() {
        for p in Parameter::ALL {
            assert_eq!(Parameter::from_name(p.name()), Some(p));
        }
        assert_eq!(Parameter::from_name("RuleFile"), None);
    }
}
