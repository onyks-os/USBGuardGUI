//! The status icon (docs/architecture.md §9.6), via `ksni`
//! (StatusNotifierItem).
//!
//! `ksni` performs no fallback of any kind: where no StatusNotifierWatcher
//! exists — GNOME without an AppIndicator extension — it reports that the
//! icon would not show, and the program continues in background mode without
//! an icon, stating so once rather than failing per attempt.
//!
//! The tray lives on the Tokio runtime. Its callbacks reach the GTK side only
//! through [`TrayCommand`] over a channel: GTK objects never cross threads.

use async_channel::Sender;
use ksni::menu::StandardItem;
use ksni::{Handle, MenuItem, ToolTip, Tray, TrayMethods};

use super::APP_ID;
use super::APP_NAME;
use super::i18n::fill;
use crate::runtime::runtime;
use gettextrs::gettext;

/// What the tray asks the GTK side to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrayCommand {
    Show,
    Quit,
}

pub(super) struct UsbguardTray {
    commands: Sender<TrayCommand>,
    total: usize,
    blocked: usize,
}

impl Tray for UsbguardTray {
    const MENU_ON_ACTIVATE: bool = false;

    fn id(&self) -> String {
        APP_ID.to_owned()
    }

    fn title(&self) -> String {
        APP_NAME.to_owned()
    }

    fn icon_name(&self) -> String {
        format!("{APP_ID}-symbolic")
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: APP_NAME.to_owned(),
            description: fill(
                &gettext("{total} USB devices, {blocked} not authorized"),
                &[
                    ("total", &self.total.to_string()),
                    ("blocked", &self.blocked.to_string()),
                ],
            ),
            ..ToolTip::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.commands.try_send(TrayCommand::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: fill(&gettext("Show {app}"), &[("app", APP_NAME)]),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.commands.try_send(TrayCommand::Show);
                }),
                ..StandardItem::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: gettext("Quit"),
                icon_name: "application-exit".to_owned(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.commands.try_send(TrayCommand::Quit);
                }),
                ..StandardItem::default()
            }
            .into(),
        ]
    }
}

/// The running tray, or why there is none.
pub(super) type TrayResult = Result<Handle<UsbguardTray>, String>;

/// Registers the icon. The answer arrives on `reply`.
pub(super) fn start(commands: Sender<TrayCommand>, reply: Sender<TrayResult>) {
    runtime().spawn(async move {
        let tray = UsbguardTray {
            commands,
            total: 0,
            blocked: 0,
        };
        let result = tray.spawn().await.map_err(|e| e.to_string());
        let _ = reply.send(result).await;
    });
}

/// Updates the tooltip counts.
pub(super) fn update(handle: &Handle<UsbguardTray>, total: usize, blocked: usize) {
    let handle = handle.clone();
    runtime().spawn(async move {
        handle
            .update(|t| {
                t.total = total;
                t.blocked = blocked;
            })
            .await;
    });
}

/// Removes the icon.
pub(super) fn stop(handle: &Handle<UsbguardTray>) {
    let handle = handle.clone();
    runtime().spawn(async move {
        handle.shutdown().await;
    });
}
