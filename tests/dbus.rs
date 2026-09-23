//! D-Bus client, worker, and supervisor tests against the mock bridge.
//!
//! The IDs in test names refer to the resilience matrix of
//! docs/architecture.md §13.1.

// Integration tests are a separate crate, outside `allow-*-in-tests`; and they
// own their runtime, so the GTK-motivated ban on `tokio::spawn` does not apply.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use support::{Mock, device_rule};
use tokio::sync::Mutex;
use usbguard_gui::dbus::coalesce::MAX_LATENCY;
use usbguard_gui::dbus::supervisor::{ServiceWait, supervise};
use usbguard_gui::dbus::worker::{WorkerExit, event_loop};
use usbguard_gui::model::{
    AccessState, AppError, DeviceId, DevicePolicy, Persistence, RemoveOutcome, RuleId, RuleTarget,
    Target, UiEvent,
};
use usbguard_gui::rules::parse_rule;

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

async fn recv(rx: &async_channel::Receiver<UiEvent>) -> UiEvent {
    tokio::time::timeout(RECV_TIMEOUT, rx.recv())
        .await
        .expect("timed out waiting for a UiEvent")
        .expect("channel closed")
}

fn rules(texts: &[&str]) -> Vec<(u32, String)> {
    texts
        .iter()
        .zip(1..)
        .map(|(t, id)| (id, (*t).to_owned()))
        .collect()
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[tokio::test]
async fn lists_devices_and_every_rule() {
    let mock = Mock::start(
        vec![(7, device_rule("allow", 1)), (9, device_rule("block", 2))],
        rules(&["allow id 1234:0001", "block label \"kiosk\""]),
    )
    .await;
    let client = mock.client().await;

    let devices = client.list_devices().await.unwrap();
    assert_eq!(devices.len(), 2);
    assert_eq!(devices[1].id, DeviceId::new(9));
    assert_eq!(devices[1].target, Target::Block);
    assert_eq!(devices[1].attrs.name.as_deref(), Some("Device 2"));

    // Phase 0: listRules takes a label; "" is "every rule".
    let rules = client.list_rules().await.unwrap();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].position, 1);
    assert!(mock.calls().contains(&"listRules \"\"".to_owned()));
}

#[tokio::test]
async fn persistence_booleans_reach_the_wire_with_the_right_polarity() {
    let mock = Mock::start(vec![(3, device_rule("block", 3))], vec![]).await;
    let client = mock.client().await;
    let id = DeviceId::new(3);

    let runtime_only = client
        .apply_device_policy(id, DevicePolicy::Allow, Persistence::RuntimeOnly)
        .await
        .unwrap();
    assert_eq!(
        runtime_only, None,
        "rule id is meaningless when not permanent"
    );

    let permanent = client
        .apply_device_policy(id, DevicePolicy::Block, Persistence::Permanent)
        .await
        .unwrap();
    assert!(permanent.is_some());

    let rule = parse_rule("allow id 1234:5678").unwrap();
    client
        .append_rule(&rule, RuleId::APPEND_LAST, Persistence::RuntimeOnly)
        .await
        .unwrap();

    let calls = mock.calls();
    assert!(calls.contains(&"applyDevicePolicy 3 0 false".to_owned()));
    assert!(calls.contains(&"applyDevicePolicy 3 1 true".to_owned()));
    // appendRule's flag is `temporary`: runtime-only is `true` there.
    assert!(calls.contains(&"appendRule allow id 1234:5678 4294967293 true".to_owned()));
}

#[tokio::test]
async fn invalid_rules_never_reach_the_daemon() {
    let mock = Mock::start(vec![], vec![]).await;
    let client = mock.client().await;
    let mut query = parse_rule("allow").unwrap();
    query.target = Some(RuleTarget::Match);
    let err = client
        .append_rule(&query, RuleId::APPEND_LAST, Persistence::Permanent)
        .await
        .unwrap_err();
    assert!(matches!(err, AppError::Parse(_)));
    assert!(mock.calls().is_empty());
}

#[tokio::test]
async fn t5_identical_rules_are_disambiguated_by_position() {
    let mock = Mock::start(
        vec![],
        rules(&["allow id 1111:2222", "block", "allow id 1111:2222"]),
    )
    .await;
    let client = mock.client().await;
    let handles = client.list_rules().await.unwrap();

    let RemoveOutcome::Ambiguous { candidates } = client.remove_rule(&handles[0]).await.unwrap()
    else {
        panic!("expected Ambiguous");
    };
    assert_eq!(
        candidates.iter().map(|c| c.position).collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert!(!mock.calls().iter().any(|c| c.starts_with("removeRule")));

    // The user picks the second one.
    assert_eq!(
        client.remove_rule_at(&candidates[1]).await.unwrap(),
        RemoveOutcome::Removed
    );
    assert_eq!(client.list_rules().await.unwrap().len(), 2);
}

#[tokio::test]
async fn t4_a_rule_removed_elsewhere_is_already_gone() {
    let mock = Mock::start(vec![], rules(&["allow id 1111:2222", "block"])).await;
    let client = mock.client().await;
    let handles = client.list_rules().await.unwrap();

    // Someone removes rule 1 with the CLI; ids shift underneath us.
    mock.state.lock().unwrap().rules.remove(0);

    assert_eq!(
        client.remove_rule(&handles[0]).await.unwrap(),
        RemoveOutcome::AlreadyGone
    );
    // No other rule was removed.
    assert_eq!(client.list_rules().await.unwrap().len(), 1);
}

#[tokio::test]
async fn polkit_denial_is_classified() {
    let mock = Mock::start(vec![], vec![]).await;
    mock.deny("listDevices");
    let err = mock.client().await.list_devices().await.unwrap_err();
    assert_eq!(
        err,
        AppError::Denied(AccessState::DeniedByPolkit { action: None })
    );
}

#[tokio::test]
async fn t6_hostile_device_name_yields_a_row() {
    let hostile = r#"allow id 1234:5678 name "quote\" back\\slash \xff\xfe end" serial "1""#;
    let broken = r#"block id 1234:5678 name "unterminated"#;
    let mock = Mock::start(
        vec![(1, hostile.to_owned()), (2, broken.to_owned())],
        vec![],
    )
    .await;
    let devices = mock.client().await.list_devices().await.unwrap();
    assert_eq!(devices.len(), 2, "no device may be hidden");
    let name = devices[0].attrs.name.as_deref().unwrap();
    assert!(name.starts_with("quote\" back\\slash "));
    assert!(name.contains('\u{fffd}'), "invalid bytes shown as U+FFFD");
    assert!(devices[1].parse_error.is_some());
    assert_eq!(devices[1].target, Target::Block);
    assert_eq!(devices[1].rule_text, broken);
}

// ---------------------------------------------------------------------------
// Worker
// ---------------------------------------------------------------------------

async fn start_worker(mock: &Mock) -> async_channel::Receiver<UiEvent> {
    let client = mock.client().await;
    let (tx, rx) = async_channel::bounded(64);
    tokio::spawn(async move { event_loop(&client, &tx).await });
    // Initial snapshot, then the rule-cache invalidation that follows it.
    assert!(matches!(recv(&rx).await, UiEvent::DeviceSnapshot(_)));
    assert_eq!(recv(&rx).await, UiEvent::InvalidateRuleCache);
    rx
}

#[tokio::test]
async fn small_bursts_are_merged_into_one_batch() {
    let mock = Mock::start(vec![], vec![]).await;
    let rx = start_worker(&mock).await;

    for n in 1..=3 {
        mock.presence(n, 1, 1, &device_rule("block", n), "d").await;
    }
    // Device 2 is authorized by the policy decision in the same window.
    mock.policy_changed(2, 0, 0).await;
    // Device 3 leaves before the window ends: it must not flicker in.
    mock.presence(3, 3, 1, "", "d").await;

    let UiEvent::DeviceBatch(batch) = recv(&rx).await else {
        panic!("expected one batch");
    };
    assert_eq!(batch.len(), 3);
    assert_eq!(batch[0].target, Some(Target::Block));
    assert_eq!(batch[1].target, Some(Target::Allow));
    assert!(batch[2].removed);
}

#[tokio::test]
async fn t3_a_forty_device_burst_resynchronizes_within_the_ceiling() {
    let mock = Mock::start(vec![], vec![]).await;
    let rx = start_worker(&mock).await;

    let burst: Vec<(u32, String)> = (1..=45).map(|n| (n, device_rule("block", n))).collect();
    mock.state.lock().unwrap().devices.clone_from(&burst);
    let started = Instant::now();
    for (id, rule) in &burst {
        mock.presence(*id, 1, 1, rule, "hub").await;
    }

    // The resync snapshot replaces the deltas; the final state is the
    // daemon's list, with zero event loss.
    let mut last_snapshot = None;
    while let Ok(Ok(event)) = tokio::time::timeout(MAX_LATENCY * 2, rx.recv()).await {
        if let UiEvent::DeviceSnapshot(devices) = event {
            last_snapshot = Some(devices);
        }
    }
    let devices = last_snapshot.expect("a resynchronization snapshot");
    assert_eq!(devices.len(), 45);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "no freeze: {:?}",
        started.elapsed()
    );
}

#[tokio::test]
async fn exceptions_are_forwarded_and_invalidate_rules() {
    let mock = Mock::start(vec![], vec![]).await;
    let rx = start_worker(&mock).await;
    mock.exception("rule parse failure").await;
    assert!(matches!(
        recv(&rx).await,
        UiEvent::DaemonException { reason, .. } if reason == "rule parse failure"
    ));
    assert_eq!(recv(&rx).await, UiEvent::InvalidateRuleCache);
}

#[tokio::test]
async fn permanent_change_by_another_client_invalidates_rules() {
    let mock = Mock::start(vec![], vec![]).await;
    let rx = start_worker(&mock).await;
    mock.policy_changed(4, 0, 17).await;
    assert_eq!(recv(&rx).await, UiEvent::InvalidateRuleCache);
    assert!(matches!(recv(&rx).await, UiEvent::DeviceBatch(_)));
}

#[tokio::test]
async fn worker_reports_ui_closed() {
    let mock = Mock::start(vec![], vec![]).await;
    let client = mock.client().await;
    let (tx, rx) = async_channel::bounded(64);
    drop(rx);
    assert_eq!(event_loop(&client, &tx).await, WorkerExit::UiClosed);
}

// ---------------------------------------------------------------------------
// Supervisor
// ---------------------------------------------------------------------------

#[tokio::test]
async fn t1_t15_bridge_restart_discards_caches_and_resynchronizes() {
    let first = Mock::start(vec![(1, device_rule("allow", 1))], vec![]).await;
    let state = first.state.clone();
    let current: Arc<Mutex<Option<Mock>>> = Arc::new(Mutex::new(Some(first)));

    let connect = {
        let (current, state) = (current.clone(), state.clone());
        move || {
            let (current, state) = (current.clone(), state.clone());
            async move {
                let mut slot = current.lock().await;
                if slot.is_none() {
                    *slot = Some(Mock::serve(state).await);
                }
                Ok(slot.as_ref().unwrap().client().await)
            }
        }
    };
    let (tx, rx) = async_channel::bounded(64);
    tokio::spawn(supervise(connect, |_| async { ServiceWait::Appeared }, tx));

    assert!(matches!(recv(&rx).await, UiEvent::DeviceSnapshot(d) if d.len() == 1));
    assert_eq!(recv(&rx).await, UiEvent::InvalidateRuleCache);

    // The bridge stops; while it is away, a device appears.
    let stopped = current.lock().await.take().unwrap();
    let started = Instant::now();
    stopped.stop().await;
    state
        .lock()
        .unwrap()
        .devices
        .push((2, device_rule("block", 2)));

    assert!(matches!(recv(&rx).await, UiEvent::ConnectionLost(_)));
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "T1: disconnected within 2 s"
    );
    assert_eq!(recv(&rx).await, UiEvent::Reconnected);
    let UiEvent::DeviceSnapshot(devices) = recv(&rx).await else {
        panic!("expected a fresh snapshot");
    };
    assert_eq!(devices.len(), 2, "no stale rows: the new device is there");
    assert_eq!(recv(&rx).await, UiEvent::InvalidateRuleCache);
}

#[tokio::test]
async fn append_after_resolves_the_anchor_by_text_at_the_moment_of_use() {
    let mock = Mock::start(vec![], rules(&["allow id 1111:0001", "block"])).await;
    let client = mock.client().await;
    let handles = client.list_rules().await.unwrap();
    let rule = parse_rule("allow id 2222:0002").unwrap();

    // Another client inserts a rule at the top: every id shifts.
    mock.state
        .lock()
        .unwrap()
        .rules
        .insert(0, (50, "reject id 9999:0009".into()));

    client
        .append_rule_after(&rule, Some(&handles[0]), Persistence::Permanent)
        .await
        .unwrap();
    let texts: Vec<String> = client
        .list_rules()
        .await
        .unwrap()
        .into_iter()
        .map(|h| h.text)
        .collect();
    assert_eq!(
        texts,
        vec![
            "reject id 9999:0009",
            "allow id 1111:0001",
            "allow id 2222:0002",
            "block"
        ]
    );

    // The anchor disappears: nothing is appended anywhere.
    mock.state
        .lock()
        .unwrap()
        .rules
        .retain(|(_, t)| t != "block");
    let gone = client
        .append_rule_after(&rule, Some(&handles[1]), Persistence::Permanent)
        .await
        .unwrap_err();
    assert_eq!(gone, AppError::Stale);
}

#[tokio::test]
async fn writes_may_prompt_for_a_password_reads_may_not() {
    // Without ALLOW_INTERACTIVE_AUTHORIZATION the bridge's Polkit check
    // refuses at once instead of asking for a password (Phase 0 finding);
    // with it on a read, a prompt could appear before any window exists.
    let mock = Mock::start(vec![(3, device_rule("block", 3))], vec![]).await;
    let client = mock.client().await;
    client.list_devices().await.unwrap();
    client.list_rules().await.unwrap();
    client
        .apply_device_policy(
            DeviceId::new(3),
            DevicePolicy::Allow,
            Persistence::RuntimeOnly,
        )
        .await
        .unwrap();
    client
        .append_rule(
            &parse_rule("allow id 1234:5678").unwrap(),
            RuleId::APPEND_LAST,
            Persistence::RuntimeOnly,
        )
        .await
        .unwrap();
    let flags = mock.state.lock().unwrap().interactive.clone();
    for (method, interactive) in flags {
        let is_write = matches!(method, "applyDevicePolicy" | "appendRule");
        assert_eq!(interactive, is_write, "{method}");
    }
}
