use std::time::{Duration, Instant};

use super::{Coalescer, DeviceSignal, MAX_LATENCY, QUIET_WINDOW, RESYNC_THRESHOLD};
use crate::model::{DeviceAttributes, DeviceEvent, DeviceId, Target};

fn presence(id: u32, event: DeviceEvent, target: Target) -> DeviceSignal {
    DeviceSignal::Presence {
        id: DeviceId::new(id),
        event,
        target,
        rule: format!("rule {id}"),
        attrs: DeviceAttributes::default(),
    }
}

fn policy(id: u32, target: Target) -> DeviceSignal {
    DeviceSignal::Policy {
        id: DeviceId::new(id),
        target,
        rule: format!("rule {id}"),
        attrs: DeviceAttributes::default(),
    }
}

#[test]
fn insert_then_remove_never_appears() {
    let now = Instant::now();
    let mut c = Coalescer::new();
    c.push(presence(1, DeviceEvent::Insert, Target::Block), now);
    c.push(presence(1, DeviceEvent::Remove, Target::Block), now);
    let batch = c.take();
    assert_eq!(batch.len(), 1);
    assert!(batch[0].removed);
    assert!(batch[0].observed_insert);
    assert!(batch[0].observed_remove);
    assert_eq!(batch[0].target, None);
}

#[test]
fn remove_then_insert_is_a_new_device() {
    let now = Instant::now();
    let mut c = Coalescer::new();
    c.push(presence(1, DeviceEvent::Remove, Target::Allow), now);
    c.push(presence(1, DeviceEvent::Insert, Target::Block), now);
    let batch = c.take();
    assert!(!batch[0].removed);
    assert!(batch[0].observed_insert);
    assert!(batch[0].observed_remove);
    assert_eq!(batch[0].target, Some(Target::Block));
}

#[test]
fn policy_target_beats_later_presence_target() {
    let now = Instant::now();
    let mut c = Coalescer::new();
    c.push(presence(2, DeviceEvent::Insert, Target::Block), now);
    c.push(policy(2, Target::Allow), now);
    c.push(presence(2, DeviceEvent::Update, Target::Block), now);
    assert_eq!(c.take()[0].target, Some(Target::Allow));
}

#[test]
fn attributes_accumulate() {
    let now = Instant::now();
    let mut c = Coalescer::new();
    let with = |name: Option<&str>, serial: Option<&str>| DeviceSignal::Presence {
        id: DeviceId::new(3),
        event: DeviceEvent::Update,
        target: Target::Allow,
        rule: String::new(),
        attrs: DeviceAttributes {
            name: name.map(Into::into),
            serial: serial.map(Into::into),
            ..DeviceAttributes::default()
        },
    };
    c.push(with(Some("old"), Some("S1")), now);
    c.push(with(Some("new"), None), now);
    let d = &c.take()[0];
    assert_eq!(d.attrs.name.as_deref(), Some("new"));
    assert_eq!(d.attrs.serial.as_deref(), Some("S1"));
}

#[test]
fn budget_respects_the_latency_ceiling() {
    let start = Instant::now();
    let mut c = Coalescer::new();
    assert_eq!(c.budget(start), None);
    c.push(presence(1, DeviceEvent::Insert, Target::Block), start);
    assert_eq!(c.budget(start), Some(QUIET_WINDOW));
    let late = start + MAX_LATENCY.saturating_sub(Duration::from_millis(30));
    assert_eq!(c.budget(late), Some(Duration::from_millis(30)));
    assert_eq!(c.budget(start + MAX_LATENCY * 2), Some(Duration::ZERO));
    c.take();
    assert_eq!(c.budget(late), None);
}

#[test]
fn resync_threshold() {
    let now = Instant::now();
    let mut c = Coalescer::new();
    for id in 0..u32::try_from(RESYNC_THRESHOLD).unwrap() {
        assert!(!c.needs_resync());
        c.push(presence(id, DeviceEvent::Insert, Target::Block), now);
    }
    assert!(c.needs_resync());
    c.clear();
    assert!(c.is_empty());
}

#[test]
fn batches_are_ordered_by_id() {
    let now = Instant::now();
    let mut c = Coalescer::new();
    for id in [9, 3, 7] {
        c.push(presence(id, DeviceEvent::Insert, Target::Block), now);
    }
    let ids: Vec<u32> = c.take().iter().map(|d| d.id.get()).collect();
    assert_eq!(ids, vec![3, 7, 9]);
}

#[test]
fn empty_coalescer_has_no_budget_or_output() {
    let mut c = Coalescer::new();
    assert!(c.is_empty());
    assert_eq!(c.budget(Instant::now()), None);
    assert_eq!(c.take(), Vec::new());
    c.push(
        presence(1, DeviceEvent::Insert, Target::Block),
        Instant::now(),
    );
    assert!(!c.is_empty());
    assert_eq!(c.len(), 1);
}
