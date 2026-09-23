//! The GTK interface (docs/architecture.md §9).
//!
//! Everything here runs on the GTK main thread. The only thing that crosses
//! into this module from the Tokio side is [`UiEvent`], over one
//! executor-agnostic channel (§5.3). GTK objects are `!Send`, so the compiler
//! itself refuses any attempt to move one onto the Tokio runtime.

// Building a widget tree is a long, flat list of declarations; splitting it
// to satisfy a line count scatters one screen over several functions.
#![allow(clippy::too_many_lines)]

mod autostart;
mod config;
mod device_view;
mod diagnostic_dialog;
mod notify;
mod policy_view;
mod preferences;
mod rule_dialog;
mod tray;
mod window;

use std::cell::RefCell;
use std::process::ExitCode;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::dbus::{Client, diagnostics, supervisor};
use crate::model::{DeviceId, Parameter, UiEvent};
use crate::runtime::runtime;
use config::Config;
use gtk::gio;
use tray::TrayCommand;
use window::MainWindow;

/// Application ID: also the D-Bus name used for single-instance behaviour
/// (§1.5). `onyks_os`, not `onyks-os`: GApplication rejects the hyphen.
pub const APP_ID: &str = "io.github.onyks_os.UsbguardGui";

/// Capacity of the event channel. Events are already coalesced (§5.4), so
/// this is never reached in practice.
const CHANNEL_CAPACITY: usize = 64;

/// Runs the application until the last window closes.
#[must_use]
pub fn run(background: bool) -> ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    let config = Config::load();
    // The flag overrides the setting (docs/interfaces.md §2).
    let background = background || config.start_in_background();

    // Built once, on the first activation; later activations (a second
    // launch, forwarded by GApplication) only present it.
    let window: Rc<RefCell<Option<MainWindow>>> = Rc::default();
    let first = Rc::new(std::cell::Cell::new(true));

    app.connect_activate({
        let window = window.clone();
        move |app| {
            let is_first = first.replace(false);
            let existing = window.borrow().clone();
            let win = existing.unwrap_or_else(|| {
                let built = build(app, config.clone());
                *window.borrow_mut() = Some(built.clone());
                built
            });
            if is_first && background {
                win.start_hidden();
            } else {
                win.present();
            }
        }
    });
    add_app_actions(&app, &window);

    // Arguments were already parsed by `cli`; GTK gets none of them.
    let status = app.run_with_args(&[std::env::args().next().unwrap_or_default()]);
    ExitCode::from(status)
}

/// Application-level actions: the targets of notification buttons and of
/// the Quit menu item, which work whether or not the window is visible.
fn add_app_actions(app: &adw::Application, window: &Rc<RefCell<Option<MainWindow>>>) {
    let show = gio::ActionEntry::builder("show-devices")
        .activate({
            let window = window.clone();
            move |_: &adw::Application, _, _| {
                if let Some(w) = window.borrow().as_ref() {
                    w.show_devices();
                }
            }
        })
        .build();
    let allow = gio::ActionEntry::builder("allow-device")
        .parameter_type(Some(&u32::static_variant_type()))
        .activate({
            let window = window.clone();
            move |_: &adw::Application, _, parameter| {
                let id = parameter.and_then(glib::Variant::get::<u32>);
                if let (Some(w), Some(id)) = (window.borrow().as_ref(), id) {
                    w.allow_from_notification(DeviceId::new(id));
                }
            }
        })
        .build();
    let quit = gio::ActionEntry::builder("quit")
        .activate(|app: &adw::Application, _, _| app.quit())
        .build();
    app.add_action_entries([show, allow, quit]);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
}

fn build(app: &adw::Application, config: Config) -> MainWindow {
    let (ui_tx, ui_rx) = async_channel::bounded::<UiEvent>(CHANNEL_CAPACITY);
    let (tray_tx, tray_rx) = async_channel::bounded::<TrayCommand>(8);
    let window = MainWindow::new(app, config, ui_tx.clone(), tray_tx);

    // Consumer: GTK side, main thread. Every widget touch happens here.
    glib::spawn_future_local({
        let window = window.clone();
        async move {
            while let Ok(event) = ui_rx.recv().await {
                window.apply(event);
            }
        }
    });
    glib::spawn_future_local({
        let (window, app) = (window.clone(), app.clone());
        async move {
            while let Ok(command) = tray_rx.recv().await {
                match command {
                    TrayCommand::Show => window.present(),
                    TrayCommand::Quit => app.quit(),
                }
            }
        }
    });

    // Producer: Tokio side. It runs in background mode too: that is what
    // lets insertions be announced with no window open.
    supervisor::start(ui_tx.clone());

    // The probe runs once the window is on screen, so any Polkit prompt it
    // causes is attributable to a window the user can see (§4.2).
    window.connect_first_map(move || probe_after_present(ui_tx.clone()));
    window
}

/// Runs the diagnostic probe on the Tokio side and reports the state.
fn probe_after_present(ui_tx: async_channel::Sender<UiEvent>) {
    runtime().spawn(async move {
        let report = diagnostics::probe().await;
        let _ = ui_tx.send(UiEvent::AccessStateChanged(report.state)).await;
    });
}

/// Reads the ruleset on the Tokio side and reports it.
fn reload_rules(ui_tx: async_channel::Sender<UiEvent>) {
    runtime().spawn(async move {
        let result = match Client::connect_system().await {
            Ok(client) => client.list_rules().await,
            Err(e) => Err(e),
        };
        let event = match result {
            Ok(rules) => UiEvent::RuleSnapshot(rules),
            Err(e) => UiEvent::RuleSnapshotFailed(e),
        };
        let _ = ui_tx.send(event).await;
    });
}

/// Re-reads the device list — after a cancelled operation, whose effect is
/// unknown (§5.7).
fn reload_devices(ui_tx: async_channel::Sender<UiEvent>) {
    runtime().spawn(async move {
        if let Ok(client) = Client::connect_system().await {
            if let Ok(devices) = client.list_devices().await {
                let _ = ui_tx.send(UiEvent::DeviceSnapshot(devices)).await;
            }
        }
    });
}

/// Reads both runtime parameters and reports them as changes.
fn reload_parameters(ui_tx: async_channel::Sender<UiEvent>) {
    runtime().spawn(async move {
        let Ok(client) = Client::connect_system().await else {
            return;
        };
        for parameter in Parameter::ALL {
            if let Ok(value) = client.get_parameter(parameter).await {
                let event = UiEvent::ParameterChanged {
                    name: parameter.name().to_owned(),
                    value,
                };
                let _ = ui_tx.send(event).await;
            }
        }
    });
}
