//! Property-based tests.
//!
//! Every function that parses or validates untrusted input belongs here. The
//! contract: for any input, the function returns a valid result or a documented
//! error — it never panics and never hangs.
//!
//! See `DYNAMIC_ANALYSIS_POLICY.md` for the triage and remediation process.

// Integration tests are a separate crate, outside `allow-expect-in-tests`.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::HashMap;

use proptest::prelude::*;
use usbguard_gui::model::{
    Attribute, AttributeName, AttributeValue, AttributeValues, Condition, ConditionClause,
    ConditionName, DeviceAttributes, DeviceEvent, DeviceId, DevicePolicy, IdPart, InterfaceType,
    Rule, RuleString, RuleTarget, SetOperator, Target, UsbId,
};
use usbguard_gui::rules::{parse_device, parse_partial, parse_query, parse_rule, render_checked};

// ---------------------------------------------------------------------------
// Generators for well-formed rules.
// ---------------------------------------------------------------------------

fn usb_id() -> impl Strategy<Value = UsbId> {
    prop_oneof![
        Just(UsbId::new(IdPart::Any, IdPart::Any)),
        any::<u16>().prop_map(|v| UsbId::new(IdPart::Value(v), IdPart::Any)),
        (any::<u16>(), any::<u16>())
            .prop_map(|(v, p)| UsbId::new(IdPart::Value(v), IdPart::Value(p))),
    ]
    .prop_map(|id| id.expect("generated ids respect the wildcard invariant"))
}

fn interface() -> impl Strategy<Value = InterfaceType> {
    prop_oneof![
        any::<u8>().prop_map(|c| InterfaceType::new(c, None, None)),
        (any::<u8>(), any::<u8>()).prop_map(|(c, s)| InterfaceType::new(c, Some(s), None)),
        (any::<u8>(), any::<u8>(), any::<u8>()).prop_map(|(c, s, p)| InterfaceType::new(
            c,
            Some(s),
            Some(p)
        )),
    ]
    .prop_map(|i| i.expect("generated interfaces respect the wildcard invariant"))
}

/// Arbitrary bytes — quotes, backslashes, control characters, and invalid
/// UTF-8 included. This is what a hostile descriptor can put in a name.
fn rule_string() -> impl Strategy<Value = RuleString> {
    prop::collection::vec(any::<u8>(), 0..24).prop_map(RuleString::from_bytes)
}

fn value_for(name: AttributeName) -> BoxedStrategy<AttributeValue> {
    match name {
        AttributeName::Id => usb_id().prop_map(AttributeValue::UsbId).boxed(),
        AttributeName::WithInterface => interface().prop_map(AttributeValue::Interface).boxed(),
        _ => rule_string().prop_map(AttributeValue::String).boxed(),
    }
}

fn set_operator() -> impl Strategy<Value = Option<SetOperator>> {
    prop::option::of(prop::sample::select(SetOperator::ALL.to_vec()))
}

fn attribute(name: AttributeName) -> impl Strategy<Value = Attribute> {
    prop_oneof![
        value_for(name).prop_map(AttributeValues::Single),
        (set_operator(), prop::collection::vec(value_for(name), 0..4))
            .prop_map(|(operator, values)| AttributeValues::Set { operator, values }),
    ]
    .prop_map(move |values| Attribute { name, values })
}

fn attributes() -> impl Strategy<Value = Vec<Attribute>> {
    // A random subset of distinct names, in random order.
    prop::sample::subsequence(AttributeName::ALL.to_vec(), 0..=AttributeName::ALL.len())
        .prop_shuffle()
        .prop_flat_map(|names| names.into_iter().map(attribute).collect::<Vec<_>>())
}

fn condition() -> impl Strategy<Value = Condition> {
    let parameterless = prop::sample::select(vec![
        ConditionName::True,
        ConditionName::False,
        ConditionName::Random,
        ConditionName::RuleApplied,
        ConditionName::RuleEvaluated,
    ])
    .prop_map(|name| (name, None));
    let with_parameter = prop_oneof![
        Just((ConditionName::LocalTime, Some(b"08:00-18:00".to_vec()))),
        Just((ConditionName::RuleApplied, Some(b"00:10:00".to_vec()))),
        Just((ConditionName::RuleEvaluated, Some(b"30".to_vec()))),
        Just((ConditionName::Random, Some(b"0.5".to_vec()))),
        Just((
            ConditionName::AllowedMatches,
            Some(b"id 1234:5678".to_vec())
        )),
    ];
    (any::<bool>(), prop_oneof![parameterless, with_parameter]).prop_map(
        |(negated, (name, parameter))| Condition {
            negated,
            name,
            parameter,
        },
    )
}

fn condition_clause() -> impl Strategy<Value = ConditionClause> {
    prop_oneof![
        condition().prop_map(ConditionClause::Single),
        (set_operator(), prop::collection::vec(condition(), 0..4)).prop_map(
            |(operator, conditions)| ConditionClause::Set {
                operator,
                conditions
            }
        ),
    ]
}

fn policy_rule() -> impl Strategy<Value = Rule> {
    (
        prop::sample::select(vec![
            RuleTarget::Allow,
            RuleTarget::Block,
            RuleTarget::Reject,
        ]),
        attributes(),
        prop::option::of(condition_clause()),
    )
        .prop_map(|(target, attributes, condition)| Rule {
            target: Some(target),
            attributes,
            condition,
            comment: None,
        })
}

// ---------------------------------------------------------------------------
// Properties.
// ---------------------------------------------------------------------------

proptest! {
    /// Rendering then parsing gives back the same rule — for every rule,
    /// including names made of arbitrary bytes.
    #[test]
    fn render_then_parse_is_identity(rule in policy_rule()) {
        let text = render_checked(&rule).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(parse_rule(&text).ok(), Some(rule));
    }

    /// The injection property (SECURITY.md, critical outcome 4): no name,
    /// however crafted, renders into text that parses as anything but one
    /// `name` attribute holding exactly those bytes.
    #[test]
    fn a_device_name_cannot_become_syntax(bytes in prop::collection::vec(any::<u8>(), 0..64)) {
        let rule = Rule {
            target: Some(RuleTarget::Allow),
            attributes: vec![Attribute {
                name: AttributeName::Name,
                values: AttributeValues::Single(AttributeValue::String(
                    RuleString::from_bytes(bytes.clone()),
                )),
            }],
            condition: None,
            comment: None,
        };
        let reparsed = parse_rule(&rule.to_string()).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(reparsed.attributes.len(), 1);
        prop_assert!(reparsed.condition.is_none());
        prop_assert!(reparsed.comment.is_none());
        prop_assert_eq!(
            reparsed.string_value(AttributeName::Name).map(RuleString::as_bytes),
            Some(bytes.as_slice())
        );
    }

    /// Arbitrary text never panics any entry point, and whatever parses
    /// re-renders to text that parses to the same thing.
    #[test]
    fn parsers_never_panic(text in ".{0,200}") {
        for rule in [parse_rule(&text), parse_partial(&text), parse_query(&text)]
            .into_iter()
            .flatten()
        {
            let rendered = rule.to_string();
            prop_assert_eq!(parse_query(&rendered).ok(), Some(rule));
        }
    }

    /// Text built from rule-language fragments reaches deeper into the parser
    /// than random characters do.
    #[test]
    fn parsers_never_panic_on_near_miss_syntax(
        parts in prop::collection::vec(
            prop::sample::select(vec![
                "allow", "block", "match", "id", "name", "with-interface", "if", "!",
                "{", "}", "(", ")", "\"", "\\", "\\x", "one-of", "match-all", "*:*",
                "12ab:*", "03:*:*", "localtime(", "allowed-matches(", "#", " ", "\"a\"",
            ]),
            0..40,
        )
    ) {
        let text: String = parts.concat();
        let _ = parse_rule(&text);
        let _ = parse_query(&text);
    }

    /// A device row is always produced, whatever the daemon sends.
    #[test]
    fn every_device_listing_yields_a_row(id in any::<u32>(), text in ".{0,200}") {
        let device = parse_device(DeviceId::new(id), text.clone());
        prop_assert_eq!(device.id.get(), id);
        prop_assert_eq!(device.rule_text, text);
    }

    /// Structured values: parse is total, and render ∘ parse is identity on
    /// what parses (modulo hex case).
    #[test]
    fn id_parsers_are_total(text in "[0-9a-fA-F*:]{0,12}") {
        if let Some(id) = UsbId::parse(text.as_bytes()) {
            prop_assert_eq!(id.to_string(), text.to_ascii_lowercase());
        }
        if let Some(i) = InterfaceType::parse(text.as_bytes()) {
            prop_assert_eq!(i.to_string(), text.to_ascii_lowercase());
        }
    }

    /// Unknown numeric values map to the fallback variant, never panic, and
    /// never become a command.
    #[test]
    fn numeric_decoding_is_total(v in any::<u32>()) {
        let target = Target::from(v);
        prop_assert_eq!(DevicePolicy::try_from(target).is_ok(), v <= 2);
        prop_assert_eq!(matches!(DeviceEvent::from(v), DeviceEvent::Other(_)), v > 3);
    }

    /// Signal attribute maps: missing keys, empty strings, and garbage give a
    /// partially populated device, never an error.
    #[test]
    fn signal_attributes_are_forgiving(
        entries in prop::collection::hash_map(
            prop::sample::select(vec![
                "id", "name", "serial", "hash", "parent-hash", "via-port",
                "with-interface", "with-connect-type", "unknown",
            ]),
            ".{0,40}",
            0..9,
        )
    ) {
        let map: HashMap<String, String> =
            entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        let attrs = DeviceAttributes::from_signal_map(&map);
        prop_assert_eq!(attrs.name.as_ref(), map.get("name"));
    }
}
