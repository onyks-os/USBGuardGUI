//! Read-only, on-demand attribute matching for the rule editor.
//!
//! Only Tokio touches D-Bus. Editing or closing cancels the request and
//! invalidates even a reply already queued for the GTK main loop.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::dbus::Client;
use crate::device_store::port_key;
use crate::model::{AppError, Device, Rule};
use crate::runtime::runtime;

use super::i18n::{self, fill};

pub(super) struct DevicePreview {
    pub(super) group: adw::PreferencesGroup,
    check: gtk::Button,
    status: gtk::Label,
    devices: gtk::Box,
    current: RefCell<Option<Rule>>,
    request: RefCell<Option<tokio::task::JoinHandle<()>>>,
    revision: Cell<u64>,
    closed: Cell<bool>,
}

impl DevicePreview {
    pub(super) fn new() -> Rc<Self> {
        let check = gtk::Button::builder()
            .label(gettext("Preview matching devices"))
            .halign(gtk::Align::Start)
            .sensitive(false)
            .build();
        let status = gtk::Label::builder().wrap(true).xalign(0.0).build();
        let devices = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.append(&check);
        content.append(&status);
        content.append(&devices);
        let group = adw::PreferencesGroup::builder()
            .title(gettext("Device preview"))
            .description(gettext(
                "Matches connected devices against this rule’s attributes. Conditions and earlier rules can change the final decision.",
            ))
            .build();
        group.add(&content);
        let preview = Rc::new(Self {
            group,
            check,
            status,
            devices,
            current: RefCell::default(),
            request: RefCell::default(),
            revision: Cell::new(0),
            closed: Cell::new(false),
        });
        preview.check.connect_clicked({
            let weak = Rc::downgrade(&preview);
            move |_| {
                if let Some(preview) = weak.upgrade() {
                    preview.activate();
                }
            }
        });
        preview.set_rule(None);
        preview
    }

    pub(super) fn set_rule(&self, rule: Option<Rule>) {
        self.cancel();
        self.check
            .set_sensitive(rule.is_some() && !self.closed.get());
        let message = if rule.is_some() {
            gettext("Preview again after editing or when connected devices change.")
        } else {
            gettext("Enter a valid rule to preview matching devices.")
        };
        self.status.set_text(&message);
        *self.current.borrow_mut() = rule;
    }

    pub(super) fn close(&self) {
        self.closed.set(true);
        self.cancel();
        self.check.set_sensitive(false);
    }

    fn cancel(&self) {
        self.revision.set(self.revision.get().wrapping_add(1));
        if let Some(task) = self.request.borrow_mut().take() {
            task.abort();
        }
        self.check.set_label(&gettext("Preview matching devices"));
        self.status.remove_css_class("error");
        while let Some(child) = self.devices.first_child() {
            self.devices.remove(&child);
        }
    }

    fn activate(self: &Rc<Self>) {
        if self.closed.get() {
            return;
        }
        if self.request.borrow().is_some() {
            self.cancel();
            self.status.set_text(&gettext("Preview cancelled."));
            return;
        }
        let Some(rule) = self.current.borrow().clone() else {
            return;
        };
        self.cancel();
        let revision = self.revision.get();
        self.check.set_label(&gettext("Cancel preview"));
        self.status
            .set_text(&gettext("Checking connected devices…"));
        let (tx, rx) = async_channel::bounded(1);
        *self.request.borrow_mut() = Some(runtime().spawn(async move {
            let result = async {
                Client::connect_system()
                    .await?
                    .preview_rule_devices(&rule)
                    .await
            }
            .await;
            let _ = tx.send(result).await;
        }));
        glib::spawn_future_local({
            let weak = Rc::downgrade(self);
            async move {
                let result = rx.recv().await;
                if let Some(preview) = weak.upgrade() {
                    preview.finish_if_current(revision, result);
                }
            }
        });
    }

    fn finish_if_current(
        &self,
        revision: u64,
        result: Result<Result<Vec<Device>, AppError>, async_channel::RecvError>,
    ) {
        if self.revision.get() != revision || self.closed.get() {
            return;
        }
        match result {
            Ok(result) => self.finish(result),
            Err(_) => self.finish(Err(AppError::Unreachable(
                "The device preview request stopped unexpectedly.".into(),
            ))),
        }
    }

    fn finish(&self, result: Result<Vec<Device>, AppError>) {
        self.request.borrow_mut().take();
        self.check.set_label(&gettext("Preview matching devices"));
        match result {
            Ok(mut devices) => {
                devices.sort_by(|a, b| {
                    port_key(a.attrs.via_port.as_deref())
                        .cmp(&port_key(b.attrs.via_port.as_deref()))
                        .then(a.id.cmp(&b.id))
                });
                self.status.set_text(&if devices.is_empty() {
                    gettext("No connected devices match these attributes in this snapshot.")
                } else {
                    fill(
                        &gettext("Matching devices in this snapshot: {count}"),
                        &[("count", &devices.len().to_string())],
                    )
                });
                for device in devices {
                    let name = device.attrs.name.as_deref().unwrap_or("—");
                    let usb_id = device
                        .attrs
                        .usb_id
                        .map_or_else(|| "—".into(), |id| id.to_string());
                    let port = device.attrs.via_port.as_deref().unwrap_or("—");
                    let text = fill(
                        &gettext("{name} · {usb_id} · port {port} · device #{id}"),
                        &[
                            ("name", name),
                            ("usb_id", &usb_id),
                            ("port", port),
                            ("id", &device.id.to_string()),
                        ],
                    );
                    let label = gtk::Label::builder()
                        .label(&text)
                        .use_markup(false)
                        .selectable(true)
                        .wrap(true)
                        .wrap_mode(gtk::pango::WrapMode::WordChar)
                        .xalign(0.0)
                        .build();
                    self.devices.append(&label);
                }
            }
            Err(error) => {
                self.status.add_css_class("error");
                self.status.set_text(&i18n::app_error(&error));
            }
        }
    }
}

impl Drop for DevicePreview {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
#[path = "../../tests/gui/rule_preview.rs"]
pub(super) mod tests;
