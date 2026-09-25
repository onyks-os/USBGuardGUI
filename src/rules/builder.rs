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

/// Why a row is invalid. Structured rather than a sentence, so the
/// interface can word it in the user's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldErrorKind {
    /// The attribute already appears in an earlier row.
    Duplicate(AttributeName),
    /// A word is not a valid value for the attribute.
    InvalidValue {
        /// Which attribute.
        name: AttributeName,
        /// The offending word, as typed.
        word: String,
    },
    /// A list attribute has no value at all.
    Empty,
    /// A set operator on a text attribute: needs the text mode.
    SetOfText,
}

/// An invalid row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    /// Index of the offending row.
    pub row: usize,
    /// What is wrong with it.
    pub kind: FieldErrorKind,
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
        let error = |kind: FieldErrorKind| FieldError { row, kind };
        if rule.attribute(field.name).is_some() {
            return Err(error(FieldErrorKind::Duplicate(field.name)));
        }
        let values = match field.name {
            AttributeName::Id | AttributeName::WithInterface => {
                let parsed: Result<Vec<AttributeValue>, &str> = field
                    .text
                    .split_whitespace()
                    .map(|word| parse_structured(field.name, word).ok_or(word))
                    .collect();
                let parsed = parsed.map_err(|word| {
                    error(FieldErrorKind::InvalidValue {
                        name: field.name,
                        word: word.to_owned(),
                    })
                })?;
                match (parsed.as_slice(), field.operator) {
                    ([], _) => return Err(error(FieldErrorKind::Empty)),
                    ([_], None) => AttributeValues::Single(
                        parsed
                            .into_iter()
                            .next()
                            .ok_or_else(|| error(FieldErrorKind::Empty))?,
                    ),
                    (_, operator) => AttributeValues::Set {
                        operator,
                        values: parsed,
                    },
                }
            }
            _ => {
                if field.operator.is_some() {
                    return Err(error(FieldErrorKind::SetOfText));
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
}
