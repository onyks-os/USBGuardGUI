use std::collections::HashMap;

use super::DeviceAttributes;

#[test]
fn signal_map_is_forgiving() {
    let mut m = HashMap::new();
    m.insert("id".into(), "not-an-id".into());
    m.insert("name".into(), "Keyboard".into());
    m.insert(
        "with-interface".into(),
        "{ 03:01:01 bogus 03:00:00 }".into(),
    );
    m.insert("unexpected".into(), "ignored".into());
    let a = DeviceAttributes::from_signal_map(&m);
    assert_eq!(a.name.as_deref(), Some("Keyboard"));
    assert!(a.usb_id.is_none());
    assert_eq!(a.with_interface.len(), 2);
    assert!(a.serial.is_none());
}

#[test]
fn merge_keeps_old_values_newer_wins() {
    let mut a = DeviceAttributes {
        name: Some("old".into()),
        serial: Some("S".into()),
        ..DeviceAttributes::default()
    };
    a.merge_from(DeviceAttributes {
        name: Some("new".into()),
        ..DeviceAttributes::default()
    });
    assert_eq!(a.name.as_deref(), Some("new"));
    assert_eq!(a.serial.as_deref(), Some("S"));
}

#[test]
fn malformed_typed_attributes_are_ignored_and_interface_updates_replace() {
    use crate::model::{
        Attribute, AttributeName, AttributeValue, AttributeValues, Device, DeviceId, InterfaceType,
        Rule, RuleString, RuleTarget, Target,
    };
    let value = AttributeValue::String(RuleString::from("wrong type"));
    let rule = Rule {
        target: Some(RuleTarget::Match),
        attributes: vec![
            Attribute {
                name: AttributeName::Id,
                values: AttributeValues::Set {
                    operator: None,
                    values: vec![value.clone()],
                },
            },
            Attribute {
                name: AttributeName::WithInterface,
                values: AttributeValues::Single(value),
            },
        ],
        ..Rule::default()
    };
    let mut attrs = DeviceAttributes::from_rule(&rule);
    assert!(attrs.usb_id.is_none());
    assert_eq!(attrs.with_interface, Vec::new());
    assert!(rule.string_value(AttributeName::WithInterface).is_some());
    let mut rule = rule;
    rule.attributes[0].values = AttributeValues::Single(AttributeValue::UsbId(
        crate::model::UsbId::parse(b"1234:5678").unwrap(),
    ));
    assert!(rule.string_value(AttributeName::Id).is_none());
    let interface = InterfaceType::parse(b"08:06:50").unwrap();
    attrs.merge_from(DeviceAttributes {
        with_interface: vec![interface],
        ..DeviceAttributes::default()
    });
    assert_eq!(attrs.with_interface, vec![interface]);
    let d = Device::from_parsed(DeviceId::new(1), "match".into(), Ok(&rule));
    assert_eq!(d.target, Target::Other(u32::MAX));
}
