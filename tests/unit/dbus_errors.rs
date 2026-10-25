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
fn typed_zbus_error_keeps_its_denial_identity() {
    let error = zbus::Error::DBus(std::sync::Arc::new(zbus::fdo::Error::AccessDenied(
        "Not authorized.".into(),
    )));
    assert_eq!(ErrorShape::from(&error), shape(DENIED, "Not authorized."));
    assert!(matches!(super::mutation_error(&error), AppError::Denied(_)));
}

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
#[test]
fn lost_write_replies_are_uncertain_but_explicit_refusals_are_not() {
    for error in [
        zbus::fdo::Error::ServiceUnknown("gone".into()),
        zbus::fdo::Error::NameHasNoOwner("gone".into()),
        zbus::fdo::Error::NoReply("gone".into()),
        zbus::fdo::Error::Disconnected("gone".into()),
        zbus::fdo::Error::SpawnChildExited("gone".into()),
    ] {
        assert!(matches!(
            super::mutation_error(&error.into()),
            AppError::Inconclusive(_)
        ));
    }
    let denied = zbus::Error::from(zbus::fdo::Error::AccessDenied("Not authorized.".into()));
    assert!(matches!(
        super::mutation_error(&denied),
        AppError::Denied(_)
    ));
    let rejected = zbus::Error::from(zbus::fdo::Error::Failed("invalid policy".into()));
    assert!(matches!(
        super::mutation_error(&rejected),
        AppError::Rejected(_)
    ));
    let local = zbus::Error::from(std::io::Error::from(std::io::ErrorKind::BrokenPipe));
    assert!(matches!(
        super::mutation_error(&local),
        AppError::Inconclusive(_)
    ));
}

#[test]
fn error_representations_and_unknown_names_keep_the_original_details() {
    let call = zbus::Message::method_call("/org/usbguard1", "getParameter")
        .build(&())
        .unwrap();
    for detail in [None, Some("New checkpoint refusal".to_owned())] {
        let error =
            zbus::Error::MethodError(DENIED.try_into().unwrap(), detail.clone(), call.clone());
        assert_eq!(
            ErrorShape::from(&error),
            shape(DENIED, detail.as_deref().unwrap_or_default())
        );
    }
    let fdo = zbus::fdo::Error::Failed("broken rule".into());
    assert_eq!(
        ErrorShape::from(&fdo),
        shape("org.freedesktop.DBus.Error.Failed", "broken rule")
    );
    let local = ErrorShape {
        name: None,
        detail: "connection closed".into(),
    };
    assert_eq!(
        classify(&local, None),
        AccessState::DeniedUnattributed {
            detail: local.detail.clone()
        }
    );
    assert_eq!(
        to_app_error(&shape("org.usbguard.NewError", "new detail"), None),
        AppError::Rejected("org.usbguard.NewError: new detail".into())
    );
    for name in [
        "org.freedesktop.DBus.Error.AuthFailed",
        "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired",
    ] {
        assert!(matches!(
            to_app_error(&shape(name, "POLKIT refused"), Some(true)),
            AppError::Denied(AccessState::DeniedByPolkit { .. })
        ));
        assert!(matches!(
            to_app_error(&shape(name, "unknown"), None),
            AppError::Denied(AccessState::DeniedUnattributed { .. })
        ));
    }
}
