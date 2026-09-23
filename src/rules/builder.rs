//! Building a rule from the guided form of the new-rule dialog
//! (docs/architecture.md §9.4): one target, then attributes one at a time,
//! each value validated for its own shape.
//!
//! Pure logic, so it is tested without a display. The dialog shows the
//! returned error next to the field it names.

use crate::model::{
    Attribute, AttributeName, AttributeValue, AttributeValues, InterfaceType, Rule, RuleString,
    RuleTarget, SetOperator, UsbId,
};

/// One attribute row of the form, as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInput {
    /// Which attribute.
    pub name: AttributeName,
    /// The set operator, if the user picked one.
    pub operator: Option<SetOperator>,
    /// The text in the value entry. For `id` and `with-interface`, several
    /// values separated by spaces; for every other attribute, one string,
    /// taken verbatim (the renderer quotes and escapes it).
    pub text: String,
}

/// Why a row is invalid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// Index of the offending row.
    pub row: usize,
    /// What to tell the user.
    pub message: String,
}

/// The expected shape of a value, for placeholder text and error messages.
#[must_use]
pub const fn value_hint(name: AttributeName) -> &'static str {
    match name {
        AttributeName::Id => "vendor:product, e.g. 1234:5678 or 1234:*",
        AttributeName::WithInterface => "class:subclass:protocol, e.g. 08:06:50 or 03:*:*",
        AttributeName::ViaPort => "port, e.g. 1-2 or 1-2.3",
        AttributeName::Hash | AttributeName::ParentHash => "hash as shown by usbguard",
        AttributeName::Name
        | AttributeName::Serial
        | AttributeName::WithConnectType
        | AttributeName::Label => "text",
    }
}

/// Builds a policy rule from the form.
///
/// # Errors
///
/// The first invalid row: a duplicate attribute, a malformed value, an empty
/// list, or a set operator on a string attribute (sets of strings need the
/// text mode, where each value is quoted separately).
pub fn build_rule(target: RuleTarget, fields: &[FieldInput]) -> Result<Rule, FieldError> {
    let mut rule = Rule {
        target: Some(target),
        ..Rule::default()
    };
    for (row, field) in fields.iter().enumerate() {
        let error = |message: String| FieldError { row, message };
        if rule.attribute(field.name).is_some() {
            return Err(error(format!(
                "“{}” is already used above",
                field.name.keyword()
            )));
        }
        let values = match field.name {
            AttributeName::Id | AttributeName::WithInterface => {
                let parsed: Result<Vec<AttributeValue>, &str> = field
                    .text
                    .split_whitespace()
                    .map(|word| parse_structured(field.name, word).ok_or(word))
                    .collect();
                let parsed = parsed.map_err(|word| {
                    error(format!(
                        "“{word}” is not valid here: expected {}",
                        value_hint(field.name)
                    ))
                })?;
                match (parsed.as_slice(), field.operator) {
                    ([], _) => return Err(error("enter at least one value".to_owned())),
                    ([_], None) => AttributeValues::Single(
                        parsed
                            .into_iter()
                            .next()
                            .ok_or_else(|| error("enter at least one value".to_owned()))?,
                    ),
                    (_, operator) => AttributeValues::Set {
                        operator,
                        values: parsed,
                    },
                }
            }
            _ => {
                if field.operator.is_some() {
                    return Err(error(
                        "a set of text values needs the text mode, where each value is quoted"
                            .to_owned(),
                    ));
                }
                AttributeValues::Single(AttributeValue::String(RuleString::from(
                    field.text.as_str(),
                )))
            }
        };
        rule.attributes.push(Attribute {
            name: field.name,
            values,
        });
    }
    Ok(rule)
}

fn parse_structured(name: AttributeName, word: &str) -> Option<AttributeValue> {
    match name {
        AttributeName::Id => UsbId::parse(word.as_bytes()).map(AttributeValue::UsbId),
        AttributeName::WithInterface => {
            InterfaceType::parse(word.as_bytes()).map(AttributeValue::Interface)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldInput, build_rule};
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
        assert!(err.message.contains("12:34"));

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
}
