//! The USBGuard rule language: parser and renderer (docs/architecture.md §7).
//!
//! Needed twice over: on read, because `listDevices` and `listRules` return
//! rule text and nothing more structured (§2.4.3); on write, so an invalid rule
//! is rejected in the dialog, before any Polkit prompt.
//!
//! ```
//! use usbguard_gui::rules::parse_rule;
//!
//! let rule = parse_rule(r#"allow id 1111:0002 name "Example Receiver""#).unwrap();
//! assert_eq!(rule.to_string(), r#"allow id 1111:0002 name "Example Receiver""#);
//! ```

// The parsing path handles untrusted input: a panic here is a denial of
// service triggered by a USB descriptor (§7.2).
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

pub mod builder;
mod lexer;
mod parser;
mod render;

pub use parser::{MAX_INPUT_LEN, MAX_SET_VALUES};
pub use render::render_checked;

use crate::model::{Device, DeviceId, ParseError, Rule};
use parser::Mode;

/// Parses policy text: the target must be `allow`, `block`, or `reject`.
///
/// # Errors
///
/// Returns a [`ParseError`] with the byte offset of the offending token.
pub fn parse_rule(input: &str) -> Result<Rule, ParseError> {
    parser::parse(input.as_bytes(), Mode::Rule)
}

/// Parses a partial rule — one with no target, as some `usbguard` CLI
/// subcommands produce.
///
/// # Errors
///
/// Returns a [`ParseError`] with the byte offset of the offending token.
pub fn parse_partial(input: &str) -> Result<Rule, ParseError> {
    parser::parse(input.as_bytes(), Mode::Partial)
}

/// Parses query text, which additionally accepts `match` as a target (§2.6).
///
/// # Errors
///
/// Returns a [`ParseError`] with the byte offset of the offending token.
pub fn parse_query(input: &str) -> Result<Rule, ParseError> {
    parser::parse(input.as_bytes(), Mode::Query)
}

/// Builds a [`Device`] from one `listDevices` entry. Never fails: rule text
/// the parser rejects still yields a row, with the raw text preserved.
#[must_use]
pub fn parse_device(id: DeviceId, rule_text: String) -> Device {
    let parsed = parse_rule(&rule_text);
    Device::from_parsed(id, rule_text, parsed.as_ref().map_err(|e| *e))
}

#[cfg(test)]
mod tests {
    use super::{parse_device, parse_partial, parse_query, parse_rule, render_checked};
    use crate::model::{
        AttributeName, AttributeValue, AttributeValues, ConditionClause, ConditionName, DeviceId,
        ParseErrorKind, RuleTarget, SetOperator, Target,
    };

    /// Parses, renders, and checks the text is reproduced exactly.
    fn round_trip(text: &str) {
        let rule = parse_query(text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert_eq!(rule.to_string(), text);
        assert_eq!(render_checked(&rule).unwrap(), text);
    }

    #[test]
    fn real_corpus_round_trips() {
        for file in [
            include_str!("../../tests/fixtures/rules/daemon-output.rules"),
            include_str!("../../tests/fixtures/rules/grammar.rules"),
        ] {
            for line in file
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
            {
                round_trip(line);
            }
        }
    }

    #[test]
    fn attribute_forms() {
        let rule = parse_rule(
            r#"block id 1234:* with-interface one-of { 03:*:* 08:06:50 } label "usb-key""#,
        )
        .unwrap();
        assert_eq!(rule.target, Some(RuleTarget::Block));
        let wi = rule.attribute(AttributeName::WithInterface).unwrap();
        let AttributeValues::Set { operator, values } = &wi.values else {
            panic!("expected a set");
        };
        assert_eq!(*operator, Some(SetOperator::OneOf));
        assert_eq!(values.len(), 2);
        assert!(matches!(values[0], AttributeValue::Interface(_)));
    }

    #[test]
    fn every_set_operator_including_match_all() {
        for op in [
            "all-of",
            "one-of",
            "none-of",
            "equals",
            "equals-ordered",
            "match-all",
        ] {
            round_trip(&format!("allow with-interface {op} {{ 03:00:00 }}"));
        }
    }

    #[test]
    fn conditions() {
        round_trip("allow if localtime(08:00-18:30:15)");
        round_trip("allow if !rule-applied(10)");
        round_trip("allow if rule-evaluated");
        round_trip("allow if random(0.25)");
        round_trip("allow if one-of { true !false random }");
        round_trip(r#"allow if allowed-matches(id 1234:5678 name "x)y")"#);
        round_trip("allow if { rule-applied(00:10:00) }");
        let rule = parse_rule("reject if !true").unwrap();
        let Some(ConditionClause::Single(c)) = rule.condition else {
            panic!("expected one condition");
        };
        assert!(c.negated);
        assert_eq!(c.name, ConditionName::True);
    }

    #[test]
    fn invalid_conditions() {
        for bad in [
            "allow if localtime",
            "allow if localtime(25)",
            "allow if true(1)",
            "allow if random(1.5)",
            "allow if rule-applied(x)",
            "allow if allowed-matches(bogus)",
        ] {
            let err = parse_rule(bad).unwrap_err();
            assert_eq!(err.kind, ParseErrorKind::InvalidConditionParameter, "{bad}");
        }
        assert_eq!(
            parse_rule("allow if sometimes").unwrap_err().kind,
            ParseErrorKind::UnknownCondition
        );
    }

    #[test]
    fn targets_per_entry_point() {
        assert_eq!(
            parse_rule("match").unwrap_err().kind,
            ParseErrorKind::InvalidTarget
        );
        assert_eq!(
            parse_rule(r#"name "x""#).unwrap_err().kind,
            ParseErrorKind::InvalidTarget
        );
        assert_eq!(
            parse_query("match").unwrap().target,
            Some(RuleTarget::Match)
        );
        assert_eq!(parse_partial(r#"name "x""#).unwrap().target, None);
        assert_eq!(
            parse_partial("allow").unwrap_err().kind,
            ParseErrorKind::InvalidTarget
        );
        assert_eq!(parse_rule("").unwrap_err().kind, ParseErrorKind::Empty);
        assert_eq!(
            parse_partial("# only").unwrap_err().kind,
            ParseErrorKind::Empty
        );
    }

    #[test]
    fn errors_point_at_the_offending_token() {
        let err = parse_rule(r#"allow name "ok" colour "red""#).unwrap_err();
        assert_eq!(
            (err.kind, err.offset),
            (ParseErrorKind::UnknownAttribute, 16)
        );
        let err = parse_rule("allow id 12345:0000").unwrap_err();
        assert_eq!((err.kind, err.offset), (ParseErrorKind::InvalidValue, 9));
        let err = parse_rule(r#"allow name "a" name "b""#).unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::DuplicateAttribute);
        let err = parse_rule("allow name").unwrap_err();
        assert_eq!((err.kind, err.offset), (ParseErrorKind::UnexpectedEnd, 10));
        let err = parse_rule("allow name unquoted").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::InvalidValue);
        let err = parse_rule("allow with-interface { 03:00:00").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnexpectedEnd);
        let err = parse_rule("allow }").unwrap_err();
        assert_eq!(err.kind, ParseErrorKind::UnexpectedToken);
    }

    #[test]
    fn hostile_device_name_cannot_inject() {
        // T6: quotes, backslashes, and non-UTF-8 bytes in a device name.
        let text = r#"allow name "a\" if true \\ \xff\xfe end" serial "1""#;
        let rule = parse_rule(text).unwrap();
        let name = rule.string_value(AttributeName::Name).unwrap();
        assert_eq!(name.as_bytes(), b"a\" if true \\ \xff\xfe end");
        assert!(rule.condition.is_none());
        let rendered = render_checked(&rule).unwrap();
        assert_eq!(parse_rule(&rendered).unwrap(), rule);
    }

    #[test]
    fn control_characters_are_escaped_on_render() {
        let rule = parse_rule(r#"allow name "a\nb\x01""#).unwrap();
        assert_eq!(rule.to_string(), r#"allow name "a\x0ab\x01""#);
    }

    #[test]
    fn device_rows_survive_bad_text() {
        let good = parse_device(
            DeviceId::new(3),
            r#"block id 1111:0001 name "Example Keyboard" via-port "1-1""#.into(),
        );
        assert_eq!(good.target, Target::Block);
        assert_eq!(good.attrs.name.as_deref(), Some("Example Keyboard"));

        let bad = parse_device(DeviceId::new(4), r#"allow name "unterminated"#.into());
        assert_eq!(bad.target, Target::Allow);
        assert!(bad.parse_error.is_some());
        assert_eq!(bad.rule_text, r#"allow name "unterminated"#);
    }

    #[test]
    fn oversized_input_is_rejected_before_parsing() {
        let huge = format!("allow name \"{}\"", "a".repeat(super::MAX_INPUT_LEN));
        assert_eq!(parse_rule(&huge).unwrap_err().kind, ParseErrorKind::TooLong);
    }

    #[test]
    fn string_values_are_typed_by_attribute() {
        let rule = parse_rule(r#"allow via-port "1-2.1" hash "abc=""#).unwrap();
        assert!(matches!(
            rule.attribute(AttributeName::ViaPort).unwrap().values,
            AttributeValues::Single(AttributeValue::String(_))
        ));
    }
}
