//! A small, privacy-preserving history of events observed by this process.
//!
//! Entries deliberately contain no device IDs, names, serials, ports, hashes,
//! rule text or D-Bus error messages. The same records are safe to export.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

const CAPACITY: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EventKind {
    Inserted,
    Removed,
    Authorized,
    Blocked,
    Rejected,
    ActionAllowed,
    ActionBlocked,
    ActionRejected,
    ActionFailed,
    ConnectionGap,
    BurstGap,
    Reconnected,
}

impl EventKind {
    const fn code(self) -> char {
        match self {
            Self::Inserted => 'i',
            Self::Removed => 'r',
            Self::Authorized => 'a',
            Self::Blocked => 'b',
            Self::Rejected => 'j',
            Self::ActionAllowed => 'A',
            Self::ActionBlocked => 'B',
            Self::ActionRejected => 'J',
            Self::ActionFailed => 'f',
            Self::ConnectionGap => 'g',
            Self::BurstGap => 'm',
            Self::Reconnected => 'c',
        }
    }

    fn from_code(code: char) -> Option<Self> {
        Some(match code {
            'i' => Self::Inserted,
            'r' => Self::Removed,
            'a' => Self::Authorized,
            'b' => Self::Blocked,
            'j' => Self::Rejected,
            'A' => Self::ActionAllowed,
            'B' => Self::ActionBlocked,
            'J' => Self::ActionRejected,
            'f' => Self::ActionFailed,
            'g' => Self::ConnectionGap,
            'm' => Self::BurstGap,
            'c' => Self::Reconnected,
            _ => return None,
        })
    }

    const fn export_name(self) -> &'static str {
        match self {
            Self::Inserted => "device_inserted",
            Self::Removed => "device_removed",
            Self::Authorized => "device_allowed",
            Self::Blocked => "device_blocked",
            Self::Rejected => "device_rejected",
            Self::ActionAllowed => "allow_action_succeeded",
            Self::ActionBlocked => "block_action_succeeded",
            Self::ActionRejected => "reject_action_succeeded",
            Self::ActionFailed => "authorization_action_failed",
            Self::ConnectionGap | Self::BurstGap => "observation_gap",
            Self::Reconnected => "connection_restored",
        }
    }

    fn label(self) -> String {
        gettext(match self {
            Self::Inserted => "USB device inserted",
            Self::Removed => "USB device removed",
            Self::Authorized => "USBGuard reports device allowed",
            Self::Blocked => "USBGuard reports device blocked",
            Self::Rejected => "USBGuard reports device rejected",
            Self::ActionAllowed => "Allow action succeeded",
            Self::ActionBlocked => "Block action succeeded",
            Self::ActionRejected => "Reject action succeeded",
            Self::ActionFailed => "Device authorization action failed",
            Self::ConnectionGap => "Observation gap: USBGuard connection lost",
            Self::BurstGap => "Observation gap: some rapid USB changes may not be listed",
            Self::Reconnected => "USBGuard connection restored; observation resumed",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct HistoryEvent {
    pub(super) timestamp: u64,
    pub(super) kind: EventKind,
}

#[derive(Debug, Clone, Default)]
pub(super) struct History {
    pub(super) events: Vec<HistoryEvent>,
}

impl History {
    pub(super) fn from_setting(value: &str) -> Self {
        let mut history = Self::default();
        for line in value.lines().rev().take(CAPACITY) {
            if line.len() > 32 {
                continue;
            }
            let Some((time, code)) = line.split_once('|') else {
                continue;
            };
            let (Ok(timestamp), Some(kind)) = (
                time.parse::<u64>(),
                code.chars().next().and_then(EventKind::from_code),
            ) else {
                continue;
            };
            history.push(HistoryEvent { timestamp, kind });
        }
        history.events.reverse();
        history
    }

    pub(super) fn record_inserted(&mut self) {
        self.record(EventKind::Inserted);
    }
    pub(super) fn record_removed(&mut self) {
        self.record(EventKind::Removed);
    }
    pub(super) fn record_reconnected(&mut self) {
        self.record(EventKind::Reconnected);
    }
    pub(super) fn record_gap(&mut self) {
        if !self.events.last().is_some_and(|event| {
            matches!(event.kind, EventKind::ConnectionGap | EventKind::BurstGap)
        }) {
            self.record(EventKind::ConnectionGap);
        }
    }

    pub(super) fn record_burst_gap(&mut self) {
        if !self.events.last().is_some_and(|event| {
            matches!(event.kind, EventKind::ConnectionGap | EventKind::BurstGap)
        }) {
            self.record(EventKind::BurstGap);
        }
    }

    pub(super) fn record_target(&mut self, target: crate::model::Target) {
        match target {
            crate::model::Target::Allow => self.record(EventKind::Authorized),
            crate::model::Target::Block => self.record(EventKind::Blocked),
            crate::model::Target::Reject => self.record(EventKind::Rejected),
            crate::model::Target::Other(_) => {}
        }
    }

    pub(super) fn record_action(&mut self, target: crate::model::DevicePolicy, success: bool) {
        if success {
            self.record(match target {
                crate::model::DevicePolicy::Allow => EventKind::ActionAllowed,
                crate::model::DevicePolicy::Block => EventKind::ActionBlocked,
                crate::model::DevicePolicy::Reject => EventKind::ActionRejected,
            });
        } else {
            self.record(EventKind::ActionFailed);
        }
    }

    fn record(&mut self, kind: EventKind) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        self.push(HistoryEvent { timestamp, kind });
    }

    fn push(&mut self, event: HistoryEvent) {
        self.events.push(event);
        if self.events.len() > CAPACITY {
            self.events.drain(..self.events.len() - CAPACITY);
        }
    }

    pub(super) fn encode(&self) -> String {
        let mut encoded = String::with_capacity(self.events.len() * 24);
        for event in &self.events {
            let _ = writeln!(encoded, "{}|{}", event.timestamp, event.kind.code());
        }
        encoded
    }

    fn export_json(&self) -> String {
        let records = self
            .events
            .iter()
            .map(|event| {
                format!(
                    "    {{\"timestamp_unix\": {}, \"event\": \"{}\"}}",
                    event.timestamp,
                    event.kind.export_name()
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        format!(
            "{{\n  \"schema_version\": 1,\n  \"identifiers_included\": false,\n  \"events\": [\n{records}\n  ]\n}}\n"
        )
    }

    fn show(&self, app: &adw::Application, parent: &impl IsA<gtk::Widget>) {
        self.show_with_cancellable(app, parent, None);
    }

    fn show_with_cancellable(
        &self,
        app: &adw::Application,
        parent: &impl IsA<gtk::Widget>,
        save_cancellable: Option<gio::Cancellable>,
    ) {
        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        for event in self.events.iter().rev() {
            let timestamp = i64::try_from(event.timestamp)
                .ok()
                .and_then(|unix| glib::DateTime::from_unix_local(unix).ok())
                .and_then(|date| date.format("%Y-%m-%d %H:%M:%S").ok())
                .map_or_else(|| event.timestamp.to_string(), |date| date.to_string());
            let row = adw::ActionRow::builder()
                .title(event.kind.label())
                .subtitle(timestamp)
                .build();
            list.append(&row);
        }
        let scrolled = gtk::ScrolledWindow::builder()
            .min_content_height(320)
            .child(&list)
            .build();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_top(18);
        content.set_margin_bottom(18);
        content.set_margin_start(18);
        content.set_margin_end(18);
        content.append(&gtk::Label::builder()
            .label(gettext("History includes events observed while USBGuardGUI is running. A marked gap means events may have been missed."))
            .wrap(true)
            .xalign(0.0)
            .build());
        content.append(&scrolled);
        let export = gtk::Button::with_label(&gettext("Export privacy-safe JSON…"));
        let transient = parent.root().and_downcast::<gtk::Window>();
        let dialog = gtk::ApplicationWindow::builder()
            .application(app)
            .title(gettext("Event history"))
            .modal(true)
            .default_width(560)
            .default_height(480)
            .child(&content)
            .build();
        dialog.set_transient_for(transient.as_ref());
        content.append(&export);
        export.connect_clicked({
            let dialog = dialog.clone();
            let json = self.export_json();
            move |_| save_export(&dialog, json.clone(), save_cancellable.as_ref())
        });
        dialog.present();
    }
}

fn save_export(
    parent: &impl IsA<gtk::Window>,
    json: String,
    cancellable: Option<&gio::Cancellable>,
) {
    let chooser = gtk::FileDialog::builder()
        .title(gettext("Export event history"))
        .accept_label(gettext("Save"))
        .build();
    chooser.set_initial_name(Some("usbguard-gui-history.json"));
    chooser.save(Some(parent), cancellable, move |result| {
        finish_export(result, &json);
    });
}

fn finish_export(result: Result<gio::File, glib::Error>, json: &str) {
    if let Ok(file) = result {
        write_export(&file, json);
    }
}

fn write_export(file: &gio::File, json: &str) {
    if let Err(error) = file.replace_contents(
        json.as_bytes(),
        None,
        false,
        gio::FileCreateFlags::REPLACE_DESTINATION,
        None::<&gio::Cancellable>,
    ) {
        tracing::warn!("could not export event history: {error}");
    }
}

pub(super) fn save_from_window(
    app: &adw::Application,
    parent: &impl IsA<gtk::Widget>,
    history: &History,
) {
    history.show(app, parent);
}

#[cfg(test)]
#[path = "../../tests/gui/history.rs"]
pub(super) mod tests;
