use super::super::tests::pump;
use super::*;
use crate::rules::parse_device;

pub(in crate::ui) fn exercise(app: &adw::Application) {
    let notifier = Notifier::new(app);
    pump();
    let mut d = parse_device(
        DeviceId::new(3),
        "block name \" Test \" hash \"hash-a\"".into(),
    );
    assert_eq!(display_name(&d), "Test");
    assert!(!notifier.still_offered(&d));
    notifier.announce(&[]);
    notifier.announce(&[d.clone()]);
    pump();
    assert!(notifier.still_offered(&d));
    let mut reused = d.clone();
    reused.attrs.hash = Some("different-device".into());
    assert!(
        !notifier.still_offered(&reused),
        "recycled ids must not authorize another device"
    );
    notifier.actions.set(Some(false));
    d.attrs.hash = None;
    d.attrs.name = Some("  ".into());
    assert_eq!(display_name(&d), "USB device 3");
    notifier.announce(&[d.clone()]);
    assert!(notifier.still_offered(&d));
    reused = d.clone();
    reused.rule_text = "block name \"new\"".into();
    assert!(!notifier.still_offered(&reused));
    notifier.announce(&[d.clone(), reused.clone()]);
    assert!(
        !notifier.still_offered(&d),
        "a group notification has no per-device allow action"
    );
    notifier.announce(&vec![d.clone(); 6]);
    notifier.withdraw();
    assert!(!notifier.still_offered(&d));
    d.rule_text = "block".into();
    assert_eq!(identity(&d), "");
}
