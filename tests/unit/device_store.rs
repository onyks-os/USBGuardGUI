use super::{DeviceStore, matches_filter, port_key};
use crate::model::{DeviceAttributes, DeviceDelta, DeviceId, PendingKind, Target};
use crate::rules::parse_device;

fn delta(id: u32) -> DeviceDelta {
    DeviceDelta {
        id: DeviceId::new(id),
        removed: false,
        target: None,
        rule_text: None,
        attrs: DeviceAttributes::default(),
    }
}

#[test]
fn batches_insert_update_and_remove() {
    let mut store = DeviceStore::new();
    store.replace(vec![parse_device(
        DeviceId::new(1),
        r#"allow id 1111:0001 name "Keyboard" via-port "1-1""#.into(),
    )]);

    let mut inserted = delta(2);
    inserted.rule_text = Some(r#"block id 2222:0001 name "Disk" via-port "1-2""#.into());
    let mut changed = delta(1);
    changed.target = Some(Target::Block);
    store.apply(vec![changed, inserted]);

    let list = store.sorted();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].target, Target::Block);
    assert_eq!(list[0].attrs.name.as_deref(), Some("Keyboard"));
    assert_eq!(list[1].attrs.name.as_deref(), Some("Disk"));

    let mut removed = delta(1);
    removed.removed = true;
    store.apply(vec![removed]);
    assert_eq!(store.len(), 1);
}

#[test]
fn pending_marks_survive_a_snapshot() {
    let mut store = DeviceStore::new();
    let device = parse_device(DeviceId::new(1), r#"block name "X""#.into());
    store.replace(vec![device.clone()]);
    store.set_pending(DeviceId::new(1), Some(PendingKind::Started));
    store.replace(vec![device]);
    assert_eq!(store.sorted()[0].pending, Some(PendingKind::Started));
}

#[test]
fn a_policy_target_in_the_delta_beats_the_rule_text() {
    let mut store = DeviceStore::new();
    let mut d = delta(5);
    d.rule_text = Some(r#"block name "X""#.into());
    d.target = Some(Target::Allow);
    store.apply(vec![d]);
    assert_eq!(store.sorted()[0].target, Target::Allow);
}

#[test]
fn ordering_and_filtering() {
    assert!(port_key(Some("usb2")) < port_key(Some("1-1")));
    assert!(port_key(Some("1-2")) < port_key(Some("1-10")));
    assert!(port_key(Some("1-2")) < port_key(None));
    let d = parse_device(
        DeviceId::new(1),
        r#"allow id 1111:0001 name "Example Keyboard" serial "AB12""#.into(),
    );
    assert!(matches_filter(&d, "keyb"));
    assert!(matches_filter(&d, "1111:"));
    assert!(matches_filter(&d, "ab12"));
    assert!(matches_filter(&d, "  "));
    assert!(!matches_filter(&d, "mouse"));
}

#[test]
fn gone_devices_and_blank_or_invalid_ports_are_safe() {
    let mut store = DeviceStore::new();
    assert!(store.is_empty());
    store.set_pending(DeviceId::new(999), Some(PendingKind::Started));
    assert!(store.is_empty());
    store.apply(vec![delta(3), delta(2)]);
    assert_eq!(
        store
            .sorted()
            .iter()
            .map(|d| d.id.get())
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
    store.clear();
    assert!(store.is_empty());
    assert_eq!(port_key(Some("  ")), port_key(None));
    assert!(port_key(Some("1-x")) > port_key(Some("1-999")));
}
