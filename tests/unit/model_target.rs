use super::{DeviceEvent, DevicePolicy, Persistence, Target};

#[test]
fn target_decoding_is_total() {
    assert_eq!(Target::from(0), Target::Allow);
    assert_eq!(Target::from(1), Target::Block);
    assert_eq!(Target::from(2), Target::Reject);
    assert_eq!(Target::from(3), Target::Other(3));
    assert_eq!(Target::from(u32::MAX), Target::Other(u32::MAX));
}

#[test]
fn unknown_target_cannot_become_a_command() {
    assert!(DevicePolicy::try_from(Target::Other(5)).is_err());
    for p in [
        DevicePolicy::Allow,
        DevicePolicy::Block,
        DevicePolicy::Reject,
    ] {
        assert_eq!(DevicePolicy::try_from(Target::from(p)), Ok(p));
        assert_eq!(Target::from(p.wire()), Target::from(p));
    }
}

#[test]
fn event_decoding_is_total() {
    assert_eq!(DeviceEvent::from(3), DeviceEvent::Remove);
    assert_eq!(DeviceEvent::from(9), DeviceEvent::Other(9));
}

#[test]
fn persistence_booleans_are_inverse() {
    for p in [Persistence::Permanent, Persistence::RuntimeOnly] {
        assert_ne!(p.as_temporary(), p.as_permanent());
    }
    assert!(Persistence::Permanent.as_permanent());
    assert!(Persistence::RuntimeOnly.as_temporary());
    assert_eq!(Persistence::default(), Persistence::RuntimeOnly);
}

#[test]
fn unknown_targets_remain_explicit_in_text() {
    for (target, text) in [
        (Target::Allow, "allowed"),
        (Target::Block, "blocked"),
        (Target::Reject, "rejected"),
        (Target::Other(91), "unknown (91)"),
    ] {
        assert_eq!(target.to_string(), text);
    }
}

#[test]
fn presence_and_update_events_remain_distinct() {
    assert_eq!(DeviceEvent::from(0), DeviceEvent::Present);
    assert_eq!(DeviceEvent::from(2), DeviceEvent::Update);
}
