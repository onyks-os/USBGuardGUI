//! Turning D-Bus errors into [`AppError`] and [`AccessState`]
//! (docs/architecture.md §4.3, §6.3).
//!
//! Classification works on a plain [`ErrorShape`] — an error name and its
//! message — rather than on `zbus::Error` directly, so every branch is unit
//! tested without having to fabricate a D-Bus reply.
//!
//! Matching on error text is fragile by construction. It is a user-experience
//! optimization, never a correctness requirement: `DeniedUnattributed` is a
//! complete, correct outcome that shows every remedy instead of guessing one.

use zbus::DBusError as _;

use crate::model::{AccessState, AppError};

/// The parts of a D-Bus error that classification looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorShape {
    /// The D-Bus error name, when the error came from the bus or a peer.
    /// `None` for local errors (I/O, handshake, …).
    pub name: Option<String>,
    /// The human-readable message.
    pub detail: String,
}

impl From<&zbus::Error> for ErrorShape {
    fn from(err: &zbus::Error) -> Self {
        match err {
            zbus::Error::MethodError(name, detail, _) => Self {
                name: Some(name.to_string()),
                detail: detail.clone().unwrap_or_default(),
            },
            zbus::Error::FDO(fdo) => Self {
                name: Some(fdo.name().to_string()),
                detail: fdo.description().unwrap_or_default().to_owned(),
            },
            other => Self {
                name: None,
                detail: other.to_string(),
            },
        }
    }
}

impl From<&zbus::fdo::Error> for ErrorShape {
    fn from(err: &zbus::fdo::Error) -> Self {
        Self {
            name: Some(err.name().to_string()),
            detail: err.description().unwrap_or_default().to_owned(),
        }
    }
}

const ACCESS_DENIED: &str = "org.freedesktop.DBus.Error.AccessDenied";
const AUTH_FAILED: &str = "org.freedesktop.DBus.Error.AuthFailed";
const INTERACTIVE_AUTH: &str = "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired";
const FAILED: &str = "org.freedesktop.DBus.Error.Failed";
const UNREACHABLE: [&str; 5] = [
    "org.freedesktop.DBus.Error.ServiceUnknown",
    "org.freedesktop.DBus.Error.NameHasNoOwner",
    "org.freedesktop.DBus.Error.NoReply",
    "org.freedesktop.DBus.Error.Disconnected",
    "org.freedesktop.DBus.Error.Spawn.ChildExited",
];

/// Phrases the bus daemon uses when its policy refuses a message (checkpoint
/// A). dbus-daemon says "Rejected send message"; dbus-broker says the sender
/// "is not allowed to send". Both are checked before the Polkit phrase, which
/// dbus-broker's wording can otherwise resemble.
const BUS_POLICY_PHRASES: [&str; 3] = [
    "rejected send message",
    "not allowed to send",
    "is not authorized to send",
];

/// Classifies a denial (§4.3). `agent_present` is the result of probe 3:
/// `Some(false)` turns a Polkit refusal into [`AccessState::NoPolkitAgent`].
#[must_use]
pub fn classify(err: &ErrorShape, agent_present: Option<bool>) -> AccessState {
    let text = err.detail.to_ascii_lowercase();
    let unattributed = || AccessState::DeniedUnattributed {
        detail: err.detail.clone(),
    };
    let Some(name) = err.name.as_deref() else {
        return unattributed();
    };

    if UNREACHABLE.contains(&name) {
        return AccessState::BridgeNotRunning {
            daemon_running: None,
        };
    }
    if is_ipc_acl_denial(name, &text) {
        return AccessState::DeniedByIpcAcl;
    }
    match name {
        ACCESS_DENIED | AUTH_FAILED | INTERACTIVE_AUTH => {
            if BUS_POLICY_PHRASES.iter().any(|p| text.contains(p)) {
                // Bus-level rejection: the message never reached the bridge.
                AccessState::DeniedByBusPolicy
            } else if text.contains("not authorized") || text.contains("polkit") {
                // usbguard-dbus answers a Polkit refusal with exactly
                // "Not authorized." (observed in Phase 0).
                if agent_present == Some(false) {
                    AccessState::NoPolkitAgent
                } else {
                    AccessState::DeniedByPolkit { action: None }
                }
            } else {
                unattributed()
            }
        }
        _ => unattributed(),
    }
}

/// The daemon's own access control (checkpoint C) surfaces through the bridge
/// as a generic `Failed` whose message is the IPC exception. The permission
/// phrases are what `IPCServer` reports; `RuleParserError` and the like are
/// ordinary rejections and must not be confused with them.
fn is_ipc_acl_denial(name: &str, lowercase_detail: &str) -> bool {
    (name == FAILED || name == ACCESS_DENIED)
        && lowercase_detail.contains("ipc")
        && ["permission", "access control", "not allowed", "denied"]
            .iter()
            .any(|p| lowercase_detail.contains(p))
}

/// Maps a D-Bus error to the error taxonomy of §6.3.
#[must_use]
pub fn to_app_error(err: &ErrorShape, agent_present: Option<bool>) -> AppError {
    let Some(name) = err.name.as_deref() else {
        return AppError::Unreachable(err.detail.clone());
    };
    if UNREACHABLE.contains(&name) {
        return AppError::Unreachable(format!("{name}: {}", err.detail));
    }
    let state = classify(err, agent_present);
    match state {
        AccessState::DeniedUnattributed { .. } if name == FAILED => {
            AppError::Rejected(err.detail.clone())
        }
        AccessState::DeniedUnattributed { .. }
            if ![ACCESS_DENIED, AUTH_FAILED, INTERACTIVE_AUTH].contains(&name) =>
        {
            AppError::Rejected(format!("{name}: {}", err.detail))
        }
        state => AppError::Denied(state),
    }
}

/// Convenience for the common case of a `zbus::Error` with no agent
/// information.
pub(crate) fn app_error(err: &zbus::Error) -> AppError {
    to_app_error(&ErrorShape::from(err), None)
}

#[cfg(test)]
mod tests {
    use super::{ErrorShape, classify, to_app_error};
    use crate::model::{AccessState, AppError};

    fn shape(name: &str, detail: &str) -> ErrorShape {
        ErrorShape {
            name: Some(name.into()),
            detail: detail.into(),
        }
    }

    const DENIED: &str = "org.freedesktop.DBus.Error.AccessDenied";

    #[test]
    fn polkit_refusal_as_the_bridge_sends_it() {
        assert_eq!(
            classify(&shape(DENIED, "Not authorized."), None),
            AccessState::DeniedByPolkit { action: None }
        );
        assert_eq!(
            classify(&shape(DENIED, "Not authorized."), Some(false)),
            AccessState::NoPolkitAgent
        );
    }

    #[test]
    fn bus_policy_is_distinguished_from_polkit() {
        // T14: dbus-daemon and dbus-broker phrasing.
        for detail in [
            "Rejected send message, 2 matched rules; type=\"method_call\"",
            "Sender :1.42 is not allowed to send to org.usbguard1",
        ] {
            assert_eq!(
                classify(&shape(DENIED, detail), Some(false)),
                AccessState::DeniedByBusPolicy,
                "{detail}"
            );
        }
    }

    #[test]
    fn ipc_acl_is_recognized_but_parser_errors_are_not() {
        let failed = "org.freedesktop.DBus.Error.Failed";
        assert_eq!(
            classify(
                &shape(
                    failed,
                    "IPC method: usbguard.IPC.listRules: permission denied"
                ),
                None
            ),
            AccessState::DeniedByIpcAcl
        );
        let rejected = shape(
            failed,
            "IPC method: usbguard.IPC.listDevices: RuleParserError",
        );
        assert_eq!(
            to_app_error(&rejected, None),
            AppError::Rejected("IPC method: usbguard.IPC.listDevices: RuleParserError".into())
        );
    }

    #[test]
    fn unreachable_names() {
        let e = shape("org.freedesktop.DBus.Error.ServiceUnknown", "gone");
        assert!(matches!(to_app_error(&e, None), AppError::Unreachable(_)));
        assert_eq!(
            classify(&e, None),
            AccessState::BridgeNotRunning {
                daemon_running: None
            }
        );
        let local = ErrorShape {
            name: None,
            detail: "I/O error: broken pipe".into(),
        };
        assert!(matches!(
            to_app_error(&local, None),
            AppError::Unreachable(_)
        ));
    }

    #[test]
    fn unknown_denials_stay_unattributed() {
        assert_eq!(
            classify(&shape(DENIED, "something new"), None),
            AccessState::DeniedUnattributed {
                detail: "something new".into()
            }
        );
        assert!(matches!(
            to_app_error(&shape(DENIED, "something new"), None),
            AppError::Denied(AccessState::DeniedUnattributed { .. })
        ));
    }

    #[test]
    fn unknown_parameter_is_a_rejection() {
        let e = shape(
            "org.freedesktop.DBus.Error.Failed",
            "getParameter: Bogus: unknown parameter",
        );
        assert!(matches!(to_app_error(&e, None), AppError::Rejected(_)));
    }
}
