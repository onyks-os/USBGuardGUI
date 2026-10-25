use super::super::tests::{Backend, button, close_dialogs, pump};
use super::*;
use crate::dbus::client::to_handles;
use crate::model::{Capabilities, DeviceDelta, RuleId};
use crate::rules::{parse_device, parse_rule};
use crate::ui::history::EventKind;
use std::time::{Duration, Instant};

fn drain(window: &MainWindow, rx: &async_channel::Receiver<UiEvent>) {
    pump();
    while let Ok(event) = rx.try_recv() {
        window.apply(event);
        pump();
    }
}
fn complete(window: &MainWindow, rx: &async_channel::Receiver<UiEvent>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !window.0.state.borrow().ops.is_empty() {
        assert!(Instant::now() < deadline, "operation did not complete");
        drain(window, rx);
    }
    drain(window, rx);
}
fn respond(window: &MainWindow, response: &str) {
    let dialog = window
        .0
        .window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AlertDialog>()
        .unwrap();
    button(&dialog, &dialog.response_label(response)).emit_clicked();
    pump();
}
#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise(app: &adw::Application, backend: &Backend) {
    let config = Config::load();
    let (tx, rx) = async_channel::bounded(64);
    let (tray_tx, _tray_rx) = async_channel::bounded(8);
    let window = MainWindow::new(app, config.clone(), tx, tray_tx);
    assert_eq!(window.0.indicator.label(), "Checking…");
    window.present();
    pump();
    let dev = parse_device(
        DeviceId::new(3),
        crate::testsupport::device_rule("block", 3),
    );
    window.apply(UiEvent::DeviceSnapshot(vec![dev.clone()]));
    window.apply(UiEvent::AccessStateChanged(AccessState::Ok {
        caps: Capabilities::default(),
    }));
    assert_eq!(window.0.indicator.label(), "Connected");
    assert!(!window.0.banner.is_revealed());
    let mapped = Rc::new(std::cell::Cell::new(0));
    window.0.window.set_visible(false);
    window.connect_first_map({
        let mapped = mapped.clone();
        move || mapped.set(mapped.get() + 1)
    });
    window.present();
    pump();
    window.0.window.set_visible(false);
    window.present();
    pump();
    assert_eq!(mapped.get(), 1);
    let focused = std::process::Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^USBGuardGUI$",
            "windowfocus",
            "--sync",
        ])
        .status()
        .unwrap();
    assert!(focused.success());
    pump();
    assert!(window.0.window.is_active());
    for text in [
        "block name \"Test insertion\"",
        "block",
        "block name \"First\"",
    ] {
        let inserted = parse_device(DeviceId::new(80), text.into());
        window.apply(UiEvent::DeviceSnapshot(vec![inserted.clone()]));
        window.announce_new(&[]);
    }
    window.apply(UiEvent::DeviceSnapshot(vec![
        dev.clone(),
        parse_device(DeviceId::new(80), "block".into()),
    ]));
    window.announce_new(&[]);
    config
        .settings()
        .unwrap()
        .set_boolean("notify-inserted", false)
        .unwrap();
    window.announce_new(&[]);
    config
        .settings()
        .unwrap()
        .set_boolean("notify-inserted", true)
        .unwrap();
    window.apply(UiEvent::DeviceSnapshot(vec![dev.clone()]));
    // Real widget callbacks must route into the window, not just the views.
    button(&window.0.policy.root, "Add Rule…").emit_clicked();
    pump();
    close_dialogs(&window.0.window);
    let status = super::super::tests::widgets::<gtk::Button>(&window.0.window)
        .into_iter()
        .find(|b| b.tooltip_text().as_deref() == Some("Show USBGuard access diagnostics"))
        .unwrap();
    status.emit_clicked();
    pump();
    close_dialogs(&window.0.window);
    window.0.banner.emit_by_name::<()>("button-clicked", &[]);
    pump();
    close_dialogs(&window.0.window);
    config.settings().unwrap().reset("tray-notice-shown");
    window.hide_to_background();
    window.present();
    pump();
    window.0.stack.set_visible_child_name(POLICY_PAGE);
    drain(&window, &rx);
    assert_eq!(window.0.policy.rules().len(), 2);
    assert!(!window.0.state.borrow().rules_stale);
    window.apply(UiEvent::RuleSnapshotFailed(AppError::Rejected(
        "<unsafe> & details".into(),
    )));
    assert!(window.0.state.borrow().rules_stale);
    window.apply(UiEvent::RuleSnapshot(to_handles(vec![(9, "allow".into())])));
    window.apply(UiEvent::InvalidateRuleCache);
    drain(&window, &rx);
    window.apply(UiEvent::ParameterChanged {
        name: "InsertedDevicePolicy".into(),
        value: "block".into(),
    });
    window.apply(UiEvent::ParameterChanged {
        name: "InsertedDevicePolicy".into(),
        value: "reject".into(),
    });
    window.apply(UiEvent::ParameterChanged {
        name: "Unknown".into(),
        value: "allow".into(),
    });
    window.apply(UiEvent::DaemonException {
        context: "IPC".into(),
        object: "test".into(),
        reason: "<reason> & plain".into(),
    });
    // All policy/persistence combinations travel through the actual client.
    for policy in [
        DevicePolicy::Allow,
        DevicePolicy::Block,
        DevicePolicy::Reject,
    ] {
        for persistence in [Persistence::RuntimeOnly, Persistence::Permanent] {
            backend.state.lock().unwrap().devices =
                vec![(3, crate::testsupport::device_rule("block", 3))];
            window.apply(UiEvent::DeviceSnapshot(vec![dev.clone()]));
            if policy == DevicePolicy::Reject {
                window.on_device_action(DeviceAction::Apply(dev.id, policy, persistence));
                respond(&window, "cancel");
                assert!(window.0.state.borrow().ops.is_empty());
                window.on_device_action(DeviceAction::Apply(dev.id, policy, persistence));
                respond(&window, "yes");
            } else {
                window.on_device_action(DeviceAction::Apply(dev.id, policy, persistence));
            }
            assert_eq!(
                window.0.state.borrow().devices.sorted()[0].target,
                Target::Block,
                "no optimistic state change"
            );
            complete(&window, &rx);
        }
    }
    backend.state.lock().unwrap().devices = vec![(3, crate::testsupport::device_rule("block", 3))];
    backend
        .state
        .lock()
        .unwrap()
        .denied
        .insert("applyDevicePolicy");
    window.start_device_op(dev.id, DevicePolicy::Allow, Persistence::RuntimeOnly);
    complete(&window, &rx);
    assert!(
        window.0.device_block.borrow().is_none(),
        "cancelled Polkit authentication must allow retry"
    );
    backend.state.lock().unwrap().denied.clear();
    window.start_set_parameter(Parameter::ImplicitPolicyTarget, "allow".into());
    complete(&window, &rx);
    window.on_policy_action(PolicyAction::SetParameter {
        parameter: Parameter::ImplicitPolicyTarget,
        old: "allow".into(),
        new: "block".into(),
    });
    respond(&window, "cancel");
    assert_eq!(
        backend.state.lock().unwrap().parameters["ImplicitPolicyTarget"],
        "allow"
    );
    window.on_policy_action(PolicyAction::SetParameter {
        parameter: Parameter::ImplicitPolicyTarget,
        old: "allow".into(),
        new: "block".into(),
    });
    respond(&window, "yes");
    complete(&window, &rx);
    backend.state.lock().unwrap().denied.insert("setParameter");
    window.start_set_parameter(Parameter::ImplicitPolicyTarget, "reject".into());
    complete(&window, &rx);
    backend.state.lock().unwrap().denied.clear();
    window.start_append(NewRule {
        rule: parse_rule("allow id 1111:2222").unwrap(),
        after: None,
        persistence: Persistence::RuntimeOnly,
    });
    complete(&window, &rx);
    let handles = window.0.policy.rules();
    let handle = handles[0].clone();
    window.on_policy_action(PolicyAction::Remove(handle.clone()));
    respond(&window, "cancel");
    window.on_policy_action(PolicyAction::Remove(handle.clone()));
    respond(&window, "yes");
    complete(&window, &rx);
    window.start_remove(handle.clone(), false);
    complete(&window, &rx); // AlreadyGone
    backend.state.lock().unwrap().rules = vec![(10, "allow".into()), (11, "allow".into())];
    let duplicate = RuleHandle {
        id: RuleId::new(10),
        text: "allow".into(),
        position: 0,
    };
    window.start_remove(duplicate, false);
    complete(&window, &rx);
    respond(&window, "pos-1");
    complete(&window, &rx);
    assert_eq!(backend.state.lock().unwrap().rules.len(), 1);
    window.ask_which_rule(Vec::new());
    respond(&window, "cancel");
    window.on_policy_action(PolicyAction::Add);
    pump();
    close_dialogs(&window.0.window);
    window.start(
        "own parameter notification".into(),
        OpKind::SetParameter {
            parameter: Parameter::InsertedDevicePolicy,
            new: "block".into(),
        },
        async {
            Ok(OperationOutcome::ParameterSet {
                previous: "apply-policy".into(),
            })
        },
    );
    window.parameter_changed("InsertedDevicePolicy", "block");
    complete(&window, &rx);
    // Exercise error outcomes through real asynchronous operation delivery.
    for (kind, error) in [
        (
            OpKind::AppendRule,
            AppError::Inconclusive("lost reply".into()),
        ),
        (OpKind::RemoveRule, AppError::Stale),
        (
            OpKind::SetParameter {
                parameter: Parameter::InsertedDevicePolicy,
                new: "block".into(),
            },
            AppError::Denied(AccessState::DeniedByIpcAcl),
        ),
        (
            OpKind::Device {
                id: dev.id,
                policy: DevicePolicy::Allow,
                persistence: Persistence::RuntimeOnly,
                name: "Test".into(),
            },
            AppError::Cancelled,
        ),
    ] {
        window.start("test fault".into(), kind, async move { Err(error) });
        complete(&window, &rx);
    }
    for state in [
        AccessState::NoPolkitAgent,
        AccessState::DeniedUnattributed {
            detail: "new denial".into(),
        },
        AccessState::DeniedByBusPolicy,
    ] {
        window.report(&AppError::Denied(state), Write::Rules);
    }
    window.report(
        &AppError::Denied(AccessState::DeniedByIpcAcl),
        Write::Devices,
    );
    assert_eq!(window.0.indicator.label(), "Read-only");
    for access in [
        AccessState::DeniedByBusPolicy,
        AccessState::BusUnavailable,
        AccessState::BridgeNotInstalled,
        AccessState::BridgeNotRunning {
            daemon_running: Some(false),
        },
    ] {
        window.apply(UiEvent::AccessStateChanged(access));
        assert!(window.0.banner.is_revealed());
        window.apply(UiEvent::DeviceSnapshot(vec![dev.clone()]));
        drain(&window, &rx);
    }
    window.apply(UiEvent::ConnectionLost("test disconnect".into()));
    assert!(!window.0.state.borrow().connected);
    assert_eq!(
        window.0.state.borrow().history.events.last().unwrap().kind,
        EventKind::ConnectionGap
    );
    window.apply(UiEvent::Reconnected);
    assert_eq!(
        window.0.state.borrow().history.events.last().unwrap().kind,
        EventKind::Reconnected
    );
    drain(&window, &rx);
    assert!(window.0.device_block.borrow().is_none());
    window.apply(UiEvent::DeviceSnapshot(vec![dev.clone()]));
    window.apply(UiEvent::DeviceBatch(vec![DeviceDelta {
        id: DeviceId::new(22),
        target: Some(Target::Block),
        rule_text: Some("block".into()),
        removed: false,
        observed_insert: true,
        observed_remove: false,
        attrs: crate::model::DeviceAttributes::default(),
    }]));
    assert!(
        window
            .0
            .state
            .borrow()
            .history
            .events
            .iter()
            .any(|event| event.kind == EventKind::Inserted)
    );
    window.allow_from_notification(DeviceId::new(500));
    pump();
    window.0.window.set_visible(false);
    window.0.notifier.announce(std::slice::from_ref(&dev));
    window.allow_from_notification(dev.id);
    complete(&window, &rx);
    window.show_devices();
    pump();
    for action in ["preferences", "about"] {
        gio::prelude::ActionGroupExt::activate_action(&window.0.window, action, None);
        pump();
        close_dialogs(&window.0.window);
    }
    gio::prelude::ActionGroupExt::activate_action(&window.0.window, "history", None);
    pump();
    let history = app
        .windows()
        .into_iter()
        .find(|candidate| candidate.title().as_deref() == Some("Event history"))
        .unwrap();
    history.close();
    pump();
    // Disabling background mode while registration is pending must win.
    let gate = std::sync::Arc::new(tokio::sync::Notify::new());
    *backend.desktop.registration_gate.lock().unwrap() = Some(gate.clone());
    config
        .settings()
        .unwrap()
        .set_boolean("run-in-background", true)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while backend.desktop.registrations.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline);
        pump();
    }
    config
        .settings()
        .unwrap()
        .set_boolean("run-in-background", false)
        .unwrap();
    gate.notify_one();
    super::super::tests::pump_for(Duration::from_millis(150));
    assert!(
        window.0.tray.borrow().is_none(),
        "late registration must not resurrect a disabled tray"
    );
    let previous = backend.desktop.registrations.lock().unwrap().len();
    let rapid = std::sync::Arc::new(tokio::sync::Notify::new());
    *backend.desktop.registration_gate.lock().unwrap() = Some(rapid.clone());
    config
        .settings()
        .unwrap()
        .set_boolean("run-in-background", true)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while backend.desktop.registrations.lock().unwrap().len() == previous {
        assert!(Instant::now() < deadline);
        pump();
    }
    config
        .settings()
        .unwrap()
        .set_boolean("run-in-background", false)
        .unwrap();
    config
        .settings()
        .unwrap()
        .set_boolean("run-in-background", true)
        .unwrap();
    rapid.notify_one();
    *backend.desktop.registration_gate.lock().unwrap() = None;
    config
        .settings()
        .unwrap()
        .set_boolean("run-in-background", true)
        .unwrap();
    pump();
    let deadline = Instant::now() + Duration::from_secs(3);
    while window.0.tray.borrow().is_none() {
        assert!(Instant::now() < deadline);
        pump();
    }
    assert!(!backend.desktop.registrations.lock().unwrap().is_empty());
    config.settings().unwrap().reset("tray-notice-shown");
    window.0.window.close();
    pump();
    assert!(!window.0.window.is_visible());
    window.start_hidden();
    assert!(window.0.hold.borrow().is_some());
    window.present();
    assert!(window.0.hold.borrow().is_none());
    config
        .settings()
        .unwrap()
        .set_boolean("run-in-background", false)
        .unwrap();
    window.0.window.close();
    pump();
    // Unknown operation IDs cannot update the UI after cancellation.
    window.apply(UiEvent::OperationFinished {
        op: OperationId(u64::MAX),
        result: Ok(OperationOutcome::RuleAppended(RuleId::new(1))),
    });
}

#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise_authentication_notices(app: &adw::Application, backend: &Backend) {
    let config = Config::load();
    let (tx, rx) = async_channel::bounded(64);
    let (tray_tx, _) = async_channel::bounded(8);
    let window = MainWindow::new(app, config, tx, tray_tx);
    let dev = parse_device(
        DeviceId::new(3),
        crate::testsupport::device_rule("block", 3),
    );
    window.apply(UiEvent::DeviceSnapshot(vec![dev.clone()]));
    window.present();
    pump();
    let gate = std::sync::Arc::new(crate::testsupport::WriteGate::default());
    backend.state.lock().unwrap().apply_gate = Some(gate.clone());
    let allow = super::super::tests::widgets::<gtk::MenuButton>(&window.0.devices.root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some("Allow"))
        .unwrap();
    allow.popup();
    pump();
    button(&allow.popover().unwrap(), "This session only").emit_clicked();
    crate::runtime::runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(3), gate.entered.notified())
            .await
            .unwrap();
    });
    let started = Instant::now();
    for (after, expected) in [
        (WAITING_NOTICE_AFTER, PendingKind::WaitingForAuthentication),
        (NO_AGENT_HINT_AFTER, PendingKind::PossiblyNoAgent),
    ] {
        eprintln!(
            "checking authentication notice after {} seconds",
            after.as_secs()
        );
        while started.elapsed() < after + Duration::from_millis(100) {
            pump();
        }
        assert_eq!(
            window.0.state.borrow().devices.sorted()[0].pending,
            Some(expected)
        );
        assert_eq!(
            window.0.state.borrow().ops.len(),
            1,
            "authentication must never auto-cancel"
        );
    }
    button(&window.0.devices.root, "Cancel").emit_clicked();
    drain(&window, &rx);
    assert!(window.0.state.borrow().ops.is_empty());
    assert!(
        window.0.state.borrow().devices.sorted()[0]
            .pending
            .is_none()
    );
    gate.resume.notify_one();
    window.on_device_action(DeviceAction::Cancel(DeviceId::new(999)));
    window.0.window.close();
    pump();
}
