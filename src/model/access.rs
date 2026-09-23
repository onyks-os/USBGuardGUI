//! The access model as data: which of the three checkpoints let us through
//! (docs/architecture.md §3–§4).
//!
//! These types live in `model`, not in `dbus::diagnostics`, because they cross
//! the executor boundary inside [`super::event::UiEvent`] and `model` must not
//! depend on zbus.

use std::fmt;

/// What the program has observed it may do (§3.5).
///
/// Established by observation, not prediction: read capabilities are set on the
/// first successful call, write capabilities start optimistically `true` and
/// are cleared by the first denial.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(clippy::struct_excessive_bools)] // six independent facts, not a state machine
pub struct Capabilities {
    /// `listDevices` succeeded.
    pub list_devices: bool,
    /// `applyDevicePolicy` has not been denied.
    pub modify_devices: bool,
    /// `listRules` succeeded.
    pub list_rules: bool,
    /// `appendRule` / `removeRule` have not been denied.
    pub modify_rules: bool,
    /// `getParameter` succeeded (the probe).
    pub get_parameters: bool,
    /// `setParameter` has not been denied.
    pub set_parameters: bool,
}

impl Default for Capabilities {
    /// Nothing read yet; every write optimistically assumed.
    fn default() -> Self {
        Self {
            list_devices: false,
            modify_devices: true,
            list_rules: false,
            modify_rules: true,
            get_parameters: false,
            set_parameters: true,
        }
    }
}

impl Capabilities {
    /// True when at least one kind of write is still believed possible.
    #[must_use]
    pub const fn can_write(self) -> bool {
        self.modify_devices || self.modify_rules || self.set_parameters
    }
}

/// Why the daemon is or is not reachable (§4.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AccessState {
    /// Reachable. `caps` says how much.
    Ok {
        /// What has been observed to work.
        caps: Capabilities,
    },
    /// No system bus in this session — usually a misconfigured container.
    BusUnavailable,
    /// The bridge package is not installed: the name is neither owned nor
    /// activatable.
    BridgeNotInstalled,
    /// The name is activatable but unowned: `usbguard-dbus.service` is not
    /// running. `daemon_running` is whether `usbguard.service` itself is
    /// active, when systemd could tell us.
    BridgeNotRunning {
        /// State of `usbguard.service`, if known.
        daemon_running: Option<bool>,
    },
    /// Rejected by the bus policy, before Polkit (checkpoint A).
    DeniedByBusPolicy,
    /// Rejected by Polkit (checkpoint B).
    DeniedByPolkit {
        /// The Polkit action, when the error named it.
        action: Option<String>,
    },
    /// Rejected by the daemon's IPC access control (checkpoint C).
    DeniedByIpcAcl,
    /// Polkit would prompt, but no authentication agent is running.
    NoPolkitAgent,
    /// Denied, and the origin could not be attributed. Every remedy is shown.
    DeniedUnattributed {
        /// The error text, for the report.
        detail: String,
    },
}

impl AccessState {
    /// A one-line summary for the header and for `--diagnose`.
    #[must_use]
    pub const fn summary(&self) -> &'static str {
        match self {
            Self::Ok { .. } => "connected",
            Self::BusUnavailable => "no system D-Bus",
            Self::BridgeNotInstalled => "the USBGuard D-Bus bridge is not installed",
            Self::BridgeNotRunning { .. } => "the USBGuard D-Bus bridge is not running",
            Self::DeniedByBusPolicy => "denied by the D-Bus bus policy",
            Self::DeniedByPolkit { .. } => "denied by Polkit",
            Self::DeniedByIpcAcl => "denied by the USBGuard IPC access control",
            Self::NoPolkitAgent => "no Polkit authentication agent is running",
            Self::DeniedUnattributed { .. } => "access denied",
        }
    }

    /// True for the four `Denied*` states and `NoPolkitAgent`.
    #[must_use]
    pub const fn is_denied(&self) -> bool {
        matches!(
            self,
            Self::DeniedByBusPolicy
                | Self::DeniedByPolkit { .. }
                | Self::DeniedByIpcAcl
                | Self::NoPolkitAgent
                | Self::DeniedUnattributed { .. }
        )
    }
}

impl fmt::Display for AccessState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.summary())?;
        match self {
            Self::DeniedByPolkit {
                action: Some(action),
            } => write!(f, " ({action})"),
            Self::DeniedUnattributed { detail } if !detail.is_empty() => write!(f, ": {detail}"),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AccessState, Capabilities};

    #[test]
    fn writes_start_optimistic_reads_pessimistic() {
        let caps = Capabilities::default();
        assert!(caps.modify_devices && caps.modify_rules && caps.set_parameters);
        assert!(!caps.list_devices && !caps.list_rules && !caps.get_parameters);
    }

    #[test]
    fn denied_classification() {
        assert!(AccessState::DeniedByIpcAcl.is_denied());
        assert!(!AccessState::BridgeNotInstalled.is_denied());
        assert!(
            !AccessState::Ok {
                caps: Capabilities::default()
            }
            .is_denied()
        );
    }
}
