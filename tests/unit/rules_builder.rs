use super::{FieldErrorKind, FieldInput, build_rule};
use crate::model::{AttributeName, RuleTarget, SetOperator};
use crate::rules::render_checked;

fn field(name: AttributeName, operator: Option<SetOperator>, text: &str) -> FieldInput {
    FieldInput {
        name,
        operator,
        text: text.to_owned(),
    }
}

#[test]
fn builds_and_renders() {
    let rule = build_rule(
        RuleTarget::Allow,
        &[
            field(AttributeName::Id, None, "1234:5678"),
            field(AttributeName::Name, None, "My \"Disk\""),
            field(
                AttributeName::WithInterface,
                Some(SetOperator::OneOf),
                "08:06:50  03:*:*",
            ),
        ],
    )
    .unwrap();
    assert_eq!(
        render_checked(&rule).unwrap(),
        r#"allow id 1234:5678 name "My \"Disk\"" with-interface one-of { 08:06:50 03:*:* }"#
    );
}

#[test]
fn several_values_without_operator_form_a_set() {
    let rule = build_rule(
        RuleTarget::Block,
        &[field(
            AttributeName::WithInterface,
            None,
            "03:00:01 03:01:01",
        )],
    )
    .unwrap();
    assert_eq!(
        rule.to_string(),
        "block with-interface { 03:00:01 03:01:01 }"
    );
}

#[test]
fn errors_name_the_row() {
    let err = build_rule(
        RuleTarget::Allow,
        &[
            field(AttributeName::Name, None, "ok"),
            field(AttributeName::Id, None, "12:34"),
        ],
    )
    .unwrap_err();
    assert_eq!(err.row, 1);
    assert_eq!(
        err.kind,
        FieldErrorKind::InvalidValue {
            name: AttributeName::Id,
            word: "12:34".to_owned()
        }
    );

    let dup = build_rule(
        RuleTarget::Allow,
        &[
            field(AttributeName::Serial, None, "a"),
            field(AttributeName::Serial, None, "b"),
        ],
    )
    .unwrap_err();
    assert_eq!(dup.row, 1);

    assert!(build_rule(RuleTarget::Allow, &[field(AttributeName::Id, None, "  ")]).is_err());
    assert!(
        build_rule(
            RuleTarget::Allow,
            &[field(AttributeName::Name, Some(SetOperator::OneOf), "x")]
        )
        .is_err()
    );
}

#[test]
fn a_bare_target_is_a_valid_rule() {
    assert_eq!(
        build_rule(RuleTarget::Reject, &[]).unwrap().to_string(),
        "reject"
    );
}
