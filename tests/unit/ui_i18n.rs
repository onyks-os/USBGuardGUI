use super::fill;

#[test]
fn placeholders_are_filled_wherever_they_are() {
    assert_eq!(fill("{b} then {a}", &[("a", "1"), ("b", "2")]), "2 then 1");
    assert_eq!(fill("no placeholders", &[("a", "1")]), "no placeholders");
}
#[test]
fn uncertain_write_explains_possible_effect_without_daemon_details() {
    let message = super::app_error(&crate::model::AppError::Inconclusive(
        "PRIVATE_SERIAL_9281".into(),
    ));
    assert!(message.contains("may already have acted"));
    assert!(message.contains("before trying again"));
    assert!(!message.contains("PRIVATE_SERIAL_9281"));
}

#[test]
fn every_structured_failure_has_a_readable_message() {
    use crate::model::{AccessState, AppError, AttributeName, ParseError, ParseErrorKind};
    use crate::rules::builder::{FieldError, FieldErrorKind};
    for state in [
        AccessState::BusUnavailable,
        AccessState::BridgeNotInstalled,
        AccessState::BridgeNotRunning {
            daemon_running: Some(false),
        },
        AccessState::DeniedByBusPolicy,
        AccessState::DeniedByPolkit { action: None },
        AccessState::DeniedByIpcAcl,
        AccessState::NoPolkitAgent,
        AccessState::DeniedUnattributed {
            detail: "private".into(),
        },
    ] {
        assert_ne!(super::access_summary(&state), "");
        assert_ne!(super::app_error(&AppError::Denied(state)), "");
    }
    for kind in [
        ParseErrorKind::TooLong,
        ParseErrorKind::TooDeep,
        ParseErrorKind::Empty,
        ParseErrorKind::UnterminatedString,
        ParseErrorKind::InvalidEscape,
        ParseErrorKind::UnterminatedParameter,
        ParseErrorKind::UnexpectedEnd,
        ParseErrorKind::UnexpectedToken,
        ParseErrorKind::InvalidTarget,
        ParseErrorKind::UnknownAttribute,
        ParseErrorKind::DuplicateAttribute,
        ParseErrorKind::InvalidValue,
        ParseErrorKind::UnknownCondition,
        ParseErrorKind::InvalidConditionParameter,
        ParseErrorKind::TooManyValues,
    ] {
        let error = ParseError::new(kind, 17);
        assert!(error.to_string().ends_with("at byte 17"));
        assert_eq!(
            super::app_error(&AppError::from(error)),
            super::parse_error(kind)
        );
    }
    for name in AttributeName::ALL {
        assert_ne!(super::value_hint(name), "");
    }
    for kind in [
        FieldErrorKind::Duplicate(AttributeName::Id),
        FieldErrorKind::InvalidValue {
            name: AttributeName::Id,
            word: "bad-id".into(),
        },
        FieldErrorKind::Empty,
        FieldErrorKind::SetOfText,
    ] {
        assert_ne!(super::field_error(&FieldError { row: 0, kind }), "");
    }
    for error in [
        AppError::Unreachable("offline".into()),
        AppError::Rejected("bad".into()),
        AppError::Cancelled,
        AppError::Stale,
    ] {
        assert_ne!(super::app_error(&error), "");
        assert_ne!(error.to_string(), "");
    }
}
