//! Desktop notifications for newly inserted, unauthorized devices
//! (docs/architecture.md §9.5).
//!
//! Sent as GIO notifications rather than with `notify-rust`: GIO already
//! speaks both the session-bus protocol and, under Flatpak, the notification
//! portal, and it routes button presses to application actions — no extra
//! dependency, and no second event loop.
//!
//! Quick-action buttons are added only when the notification server
//! advertises the `actions` capability. A notification carrying buttons that
//! the server silently drops is worse than one that never claimed to have
//! them; without them, clicking the notification opens the window.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::model::{Device, DeviceId};
use crate::runtime::runtime;

/// One notification id: a new burst replaces the previous notification
/// instead of stacking thirty of them.
const NOTIFICATION_ID: &str = "inserted";

/// What identifies a device beyond its daemon id, which is recycled (§2.4.1):
/// its descriptor hash, or failing that its rule text without the target.
fn identity(device: &Device) -> String {
    device.attrs.hash.clone().unwrap_or_else(|| {
        device
            .rule_text
            .split_once(' ')
            .map_or_else(String::new, |(_, rest)| rest.to_owned())
    })
}

fn display_name(device: &Device) -> String {
    device
        .attrs
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map_or_else(|| format!("USB device {}", device.id), str::to_owned)
}

pub(super) struct Notifier {
    app: adw::Application,
    /// `Some(false)` once the server said it has no `actions` capability.
    actions: Rc<Cell<Option<bool>>>,
    /// The identity of each device a notification offered to allow.
    offered: RefCell<HashMap<DeviceId, String>>,
}

impl Notifier {
    pub(super) fn new(app: &adw::Application) -> Self {
        let actions: Rc<Cell<Option<bool>>> = Rc::default();
        let (tx, rx) = async_channel::bounded(1);
        runtime().spawn(async move {
            let _ = tx.send(server_supports_actions().await).await;
        });
        glib::spawn_future_local({
            let actions = actions.clone();
            async move {
                if let Ok(supported) = rx.recv().await {
                    actions.set(supported);
                }
            }
        });
        Self {
            app: app.clone(),
            actions,
            offered: RefCell::default(),
        }
    }

    /// Announces devices that just appeared and are not authorized.
    pub(super) fn announce(&self, devices: &[Device]) {
        let Some(first) = devices.first() else {
            return;
        };
        let notification = if let [only] = devices {
            let n = gio::Notification::new("USB device blocked");
            n.set_body(Some(&format!(
                "“{}” was plugged in and is not authorized.",
                display_name(only)
            )));
            if self.actions.get() != Some(false) {
                n.add_button_with_target_value(
                    "Allow for this session",
                    "app.allow-device",
                    Some(&only.id.get().to_variant()),
                );
            }
            n
        } else {
            let n = gio::Notification::new(&format!("{} USB devices blocked", devices.len()));
            let names: Vec<String> = devices.iter().take(4).map(display_name).collect();
            let more = devices.len().saturating_sub(names.len());
            let body = if more > 0 {
                format!("{} and {more} more", names.join(", "))
            } else {
                names.join(", ")
            };
            n.set_body(Some(&body));
            n
        };
        notification.set_icon(&gio::ThemedIcon::new(super::APP_ID));
        notification.set_default_action("app.show-devices");
        notification.set_priority(gio::NotificationPriority::High);

        let mut offered = self.offered.borrow_mut();
        offered.clear();
        if devices.len() == 1 {
            offered.insert(first.id, identity(first));
        }
        self.app
            .send_notification(Some(NOTIFICATION_ID), &notification);
    }

    /// Whether `device` is still the device the notification offered. Device
    /// ids are recycled: a late click must not authorize whatever device now
    /// holds the id.
    pub(super) fn still_offered(&self, device: &Device) -> bool {
        self.offered
            .borrow()
            .get(&device.id)
            .is_some_and(|expected| *expected == identity(device))
    }

    pub(super) fn withdraw(&self) {
        self.offered.borrow_mut().clear();
        self.app.withdraw_notification(NOTIFICATION_ID);
    }
}

/// `org.freedesktop.Notifications.GetCapabilities` contains `actions`?
/// `None` when the server cannot be asked (for example inside a sandbox,
/// where the portal handles buttons itself).
async fn server_supports_actions() -> Option<bool> {
    let connection = zbus::Connection::session().await.ok()?;
    let proxy = zbus::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    )
    .await
    .ok()?;
    let caps: Vec<String> = proxy.call("GetCapabilities", &()).await.ok()?;
    Some(caps.iter().any(|c| c == "actions"))
}
