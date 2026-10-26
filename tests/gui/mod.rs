//! GTK contracts run together on one thread, with a private bus and display.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::too_many_lines)]

use adw::prelude::*;
use gtk::{gio, glib};
use std::time::{Duration, Instant};

pub(super) fn pump() {
    pump_for(Duration::from_millis(30));
}
pub(super) fn pump_for(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
pub(super) fn widgets<T: IsA<gtk::Widget> + glib::object::ObjectType>(
    root: &impl IsA<gtk::Widget>,
) -> Vec<T> {
    fn visit<T: IsA<gtk::Widget> + glib::object::ObjectType>(
        root: &gtk::Widget,
        found: &mut Vec<T>,
    ) {
        if let Ok(widget) = root.clone().downcast::<T>() {
            found.push(widget);
        }
        let mut child = root.first_child();
        while let Some(widget) = child {
            visit(&widget, found);
            child = widget.next_sibling();
        }
    }
    let mut found = Vec::new();
    visit(root.as_ref(), &mut found);
    found
}
pub(super) fn button(root: &impl IsA<gtk::Widget>, label: &str) -> gtk::Button {
    widgets::<gtk::Button>(root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some(label))
        .unwrap_or_else(|| panic!("missing button {label}"))
}
pub(super) fn close_dialogs(window: &adw::ApplicationWindow) {
    while let Some(dialog) = window.visible_dialog() {
        dialog.force_close();
        pump();
    }
}

#[test]
#[ignore = "requires an isolated session/system bus, virtual display and memory GSettings"]
fn gui_application_contracts() {
    assert_eq!(std::env::var("USBGUARD_GUI_BUS").as_deref(), Ok("session"));
    assert_eq!(std::env::var("GSETTINGS_BACKEND").as_deref(), Ok("memory"));
    gtk::init().unwrap();
    adw::init().unwrap();
    super::i18n::init();
    let app = adw::Application::builder()
        .application_id(super::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    super::preferences::tests::exercise_native_close(&app);
    let backend = Backend::start();
    super::config::tests::exercise();
    super::history::tests::exercise(&app);
    super::rule_dialog::tests::exercise(&app);
    super::rule_preview::tests::exercise(&backend);
    super::policy_view::tests::exercise();
    super::device_view::tests::exercise();
    super::notify::tests::exercise(&app);
    super::preferences::tests::exercise(&app);
    super::diagnostic_dialog::tests::exercise(&app, &backend);
    super::autostart::tests::exercise();
    super::window::tests::exercise(&app, &backend);
    pump();
}

pub(super) struct Backend {
    pub state: crate::testsupport::Shared,
    pub connection: zbus::Connection,
    pub desktop: std::sync::Arc<DesktopState>,
}
impl Backend {
    pub(super) fn start() -> Self {
        use crate::testsupport::{Devices, Policy, Root, State};
        use std::sync::{Arc, Mutex};
        let state = Arc::new(Mutex::new(State {
            parameters: [
                ("ImplicitPolicyTarget".into(), "block".into()),
                ("InsertedDevicePolicy".into(), "apply-policy".into()),
            ]
            .into(),
            devices: vec![(3, crate::testsupport::device_rule("block", 3))],
            rules: vec![(1, "allow id 1234:5678".into()), (2, "block".into())],
            next_rule_id: 10,
            ..State::default()
        }));
        let desktop = Arc::new(DesktopState::default());
        desktop.capabilities.lock().unwrap().push("actions".into());
        let connection = crate::runtime::runtime().block_on(async {
            zbus::connection::Builder::session()
                .name("org.usbguard1")
                .name("org.freedesktop.Notifications")
                .name("org.kde.StatusNotifierWatcher")
                .serve_at(
                    "/org/freedesktop/Notifications",
                    Notifications(desktop.clone()),
                )
                .serve_at("/StatusNotifierWatcher", Watcher(desktop.clone()))
                .serve_at("/org/usbguard1", Root(state.clone()))
                .serve_at("/org/usbguard1/Devices", Devices(state.clone()))
                .serve_at("/org/usbguard1/Policy", Policy(state.clone()))
                .build()
                .await
                .unwrap()
        });
        Self {
            state,
            connection,
            desktop,
        }
    }
}

#[derive(Default)]
pub(super) struct DesktopState {
    pub notifications: std::sync::Mutex<Vec<(String, String, Vec<String>)>>,
    pub registrations: std::sync::Mutex<Vec<String>>,
    pub capabilities: std::sync::Mutex<Vec<String>>,
    pub registration_gate: std::sync::Mutex<Option<std::sync::Arc<tokio::sync::Notify>>>,
}
struct Notifications(std::sync::Arc<DesktopState>);
#[allow(
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    clippy::unused_self
)]
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    fn get_capabilities(&self) -> Vec<String> {
        self.0.capabilities.lock().unwrap().clone()
    }
    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("Test notifications", "USBGuardGUI", "1", "1.2")
    }
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: std::collections::HashMap<String, zbus::OwnedVariant>,
        expire_timeout: i32,
    ) -> u32 {
        let _ = (app_name, replaces_id, app_icon, hints, expire_timeout);
        self.0
            .notifications
            .lock()
            .unwrap()
            .push((summary.into(), body.into(), actions));
        1
    }
    fn close_notification(&self, id: u32) {
        let _ = id;
    }
}
struct Watcher(std::sync::Arc<DesktopState>);
#[allow(clippy::unused_self)]
#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(&self, service: &str) {
        self.0.registrations.lock().unwrap().push(service.into());
        let gate = self.0.registration_gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.notified().await;
        }
    }
    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }
}

#[test]
#[ignore = "requires an isolated desktop and a real 180-second authentication wait"]
fn gui_authentication_notices() {
    assert_eq!(std::env::var("USBGUARD_GUI_BUS").as_deref(), Ok("session"));
    assert_eq!(std::env::var("GSETTINGS_BACKEND").as_deref(), Ok("memory"));
    gtk::init().unwrap();
    adw::init().unwrap();
    super::i18n::init();
    let backend = Backend::start();
    let app = adw::Application::builder()
        .application_id(super::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    super::window::tests::exercise_authentication_notices(&app, &backend);
}

#[test]
fn unreachable_backend_read_helpers_report_failure() {
    const MARKER: &str = "USBGUARD_GUI_UNREACHABLE_TEST_CHILD";
    if std::env::var_os(MARKER).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "ui::tests::unreachable_backend_read_helpers_report_failure",
                "--nocapture",
            ])
            .env(MARKER, "1")
            .env("USBGUARD_GUI_BUS", "system")
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                "unix:path=/tmp/usbguard-gui-no-such-bus",
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let (tx, rx) = async_channel::unbounded();
    super::reload_rules(tx.clone());
    super::reload_devices(tx.clone());
    super::reload_parameters(tx.clone());
    super::probe_after_present(tx.clone());
    let worker = crate::dbus::supervisor::start(tx);
    crate::runtime::runtime().block_on(async {
        let mut rules_failed = false;
        let mut diagnosed = false;
        let mut lost = false;
        while !(rules_failed && diagnosed && lost) {
            match tokio::time::timeout(Duration::from_secs(3), rx.recv())
                .await
                .unwrap()
                .unwrap()
            {
                crate::model::UiEvent::RuleSnapshotFailed(_) => rules_failed = true,
                crate::model::UiEvent::AccessStateChanged(
                    crate::model::AccessState::BusUnavailable,
                ) => diagnosed = true,
                crate::model::UiEvent::ConnectionLost(_) => lost = true,
                other => panic!("unexpected event {other:?}"),
            }
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(rx);
        tokio::time::timeout(Duration::from_secs(1), worker)
            .await
            .unwrap()
            .unwrap();
    });
}
