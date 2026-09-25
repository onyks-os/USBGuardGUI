//! The device list (docs/architecture.md §9.2).
//!
//! A column view over a list of devices. Sorting is by port, done by
//! [`DeviceStore::sorted`](crate::device_store::DeviceStore::sorted) before
//! the rows are handed to GTK; filtering is a GTK filter over that list.
//!
//! Per-row actions — Allow, Block, Reject — each open a choice between *this
//! session only* and *permanently*. A row with an operation in flight shows a
//! spinner and a Cancel button in place of its actions (§5.7).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use gettextrs::gettext;

use super::config::Config;
use super::i18n::fill;
use crate::device_store::matches_filter;
use crate::model::{Device, DeviceId, DevicePolicy, PendingKind, Persistence, Target};

/// What a row asks the window to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeviceAction {
    /// Allow, block, or reject a device.
    Apply(DeviceId, DevicePolicy, Persistence),
    /// Stop waiting for the operation in flight on this device.
    Cancel(DeviceId),
}

/// Receives row actions.
pub(super) type DeviceHandler = Rc<dyn Fn(DeviceAction)>;

/// `None` when devices may be modified; otherwise why not, for the tooltip
/// of the insensitive buttons (§3.5: never present-and-failing).
pub(super) type ModifyBlock = Rc<RefCell<Option<String>>>;

/// Icon and text for a state: never colour alone (§9.7).
fn state_presentation(target: Target) -> (&'static str, String) {
    match target {
        // Translators: the authorization state of a USB device.
        Target::Allow => ("emblem-ok-symbolic", gettext("Allowed")),
        // Translators: the authorization state of a USB device.
        Target::Block => ("action-unavailable-symbolic", gettext("Blocked")),
        // Translators: the authorization state of a USB device.
        Target::Reject => ("process-stop-symbolic", gettext("Rejected")),
        // Translators: the authorization state of a USB device.
        Target::Other(_) => ("dialog-question-symbolic", gettext("Unknown")),
    }
}

fn pending_text(kind: PendingKind) -> String {
    match kind {
        PendingKind::Started => gettext("Working…"),
        PendingKind::WaitingForAuthentication => gettext("Waiting for authentication"),
        PendingKind::PossiblyNoAgent => gettext("Still waiting — is a Polkit agent running?"),
    }
}

/// Absent or blank attributes render as an explicit dash (§6.2).
fn dash(value: Option<&str>) -> String {
    match value.map(str::trim) {
        Some(v) if !v.is_empty() => v.to_owned(),
        _ => "—".to_owned(),
    }
}

/// The devices page.
#[derive(Clone)]
pub(super) struct DeviceView {
    /// The page's root widget.
    pub(super) root: gtk::Box,
    store: gio::ListStore,
    filter: gtk::CustomFilter,
    content: gtk::Stack,
    list_box: gtk::Box,
}

impl DeviceView {
    pub(super) fn new(config: &Config, handler: DeviceHandler, block: ModifyBlock) -> Self {
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();

        let query: Rc<RefCell<String>> = Rc::default();
        let blocked_only = gtk::ToggleButton::builder()
            .label(gettext("Blocked only"))
            .tooltip_text(gettext("Show only devices that are not authorized"))
            .build();
        if let Some(settings) = config.settings() {
            settings
                .bind("show-blocked-only", &blocked_only, "active")
                .build();
        }

        let filter = gtk::CustomFilter::new({
            let query = query.clone();
            let blocked_only = blocked_only.clone();
            move |obj| {
                let Some(obj) = obj.downcast_ref::<glib::BoxedAnyObject>() else {
                    return false;
                };
                let device = obj.borrow::<Device>();
                (!blocked_only.is_active() || device.target != Target::Allow)
                    && matches_filter(&device, &query.borrow())
            }
        });
        let filtered = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));

        let search = gtk::SearchEntry::builder()
            .placeholder_text(gettext("Filter by name, ID, or serial"))
            .hexpand(true)
            .build();
        search.connect_search_changed({
            let (query, filter) = (query, filter.clone());
            move |entry| {
                *query.borrow_mut() = entry.text().to_string();
                filter.changed(gtk::FilterChange::Different);
            }
        });
        blocked_only.connect_toggled({
            let filter = filter.clone();
            move |_| filter.changed(gtk::FilterChange::Different)
        });

        let columns = gtk::ColumnView::builder()
            .model(&gtk::NoSelection::new(Some(filtered)))
            .show_row_separators(true)
            .reorderable(false)
            .build();
        columns.add_css_class("data-table");
        columns.append_column(&state_column());
        for (title, expand, text) in text_columns() {
            columns.append_column(&text_column(&title, expand, text));
        }
        columns.append_column(&actions_column(config.clone(), handler, block));

        let toolbar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();
        toolbar.append(&search);
        toolbar.append(&blocked_only);

        let list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list_box.append(&toolbar);
        list_box.append(
            &gtk::ScrolledWindow::builder()
                .child(&columns)
                .vexpand(true)
                .build(),
        );

        let content = gtk::Stack::new();
        content.add_named(
            &adw::StatusPage::builder()
                .icon_name("content-loading-symbolic")
                .title(gettext("Connecting to USBGuard…"))
                .build(),
            Some("loading"),
        );
        content.add_named(
            &adw::StatusPage::builder()
                .icon_name("drive-removable-media-symbolic")
                .title(gettext("No USB devices"))
                .description(gettext("USBGuard reports no connected devices."))
                .build(),
            Some("empty"),
        );
        content.add_named(&list_box, Some("list"));
        content.set_visible_child_name("loading");
        content.set_vexpand(true);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&content);

        Self {
            root,
            store,
            filter,
            content,
            list_box,
        }
    }

    /// Replaces the rows. `devices` is already in presentation order.
    pub(super) fn set_devices(&self, devices: Vec<Device>) {
        let empty = devices.is_empty();
        let objects: Vec<glib::BoxedAnyObject> =
            devices.into_iter().map(glib::BoxedAnyObject::new).collect();
        self.store.splice(0, self.store.n_items(), &objects);
        self.filter.changed(gtk::FilterChange::Different);
        self.content
            .set_visible_child_name(if empty { "empty" } else { "list" });
    }

    /// While disconnected the rows no longer describe the system: they are
    /// greyed out rather than presented as current.
    pub(super) fn set_stale(&self, stale: bool) {
        self.list_box.set_sensitive(!stale);
    }

    /// Back to "Connecting…", with no rows: used after a reconnection, until
    /// the fresh snapshot arrives.
    pub(super) fn reset(&self) {
        self.store.remove_all();
        self.content.set_visible_child_name("loading");
    }
}

type TextOf = fn(&Device) -> String;

/// Name, ID, serial, port. The daemon's device id is in the tooltip only:
/// next to `vendor:product` it would read as a second identifier of the same
/// kind, which it is not (§2.4.1).
fn text_columns() -> [(String, bool, TextOf); 4] {
    [
        (gettext("Name"), true, |d| {
            if d.parse_error.is_some() {
                fill(&gettext("(unreadable) {rule}"), &[("rule", &d.rule_text)])
            } else {
                dash(d.attrs.name.as_deref())
            }
        }),
        (gettext("ID"), false, |d| {
            d.attrs
                .usb_id
                .map_or_else(|| "—".to_owned(), |id| id.to_string())
        }),
        (gettext("Serial"), false, |d| {
            dash(d.attrs.serial.as_deref())
        }),
        (gettext("Port"), false, |d| {
            dash(d.attrs.via_port.as_deref())
        }),
    ]
}

fn tooltip(device: &Device) -> String {
    let mut text = fill(
        &gettext("Daemon device id: {id}"),
        &[("id", &device.id.to_string())],
    );
    text.push('\n');
    text.push_str(&device.rule_text);
    if let Some(err) = device.parse_error {
        text.push_str("\n\n");
        text.push_str(&gettext("Could not be parsed:"));
        text.push(' ');
        text.push_str(&super::i18n::parse_error(err.kind));
    }
    text
}

/// The device bound to a list item right now, if any.
fn bound_device(item: &gtk::ListItem) -> Option<Device> {
    let obj = item.item().and_downcast::<glib::BoxedAnyObject>()?;
    let device = obj.borrow::<Device>().clone();
    Some(device)
}

fn text_column(title: &str, expand: bool, text: TextOf) -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let (Some(label), Some(device)) = (
            item.child().and_downcast::<gtk::Label>(),
            bound_device(item),
        ) else {
            return;
        };
        // Plain text, never markup: device names come from the device.
        label.set_text(&text(&device));
        label.set_tooltip_text(Some(&tooltip(&device)));
    });
    let column = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    column.set_expand(expand);
    column.set_resizable(true);
    column
}

fn state_column() -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let cell = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        cell.append(&gtk::Image::new());
        cell.append(&gtk::Label::new(None));
        item.set_child(Some(&cell));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let (Some(cell), Some(device)) =
            (item.child().and_downcast::<gtk::Box>(), bound_device(item))
        else {
            return;
        };
        let (icon, text) = state_presentation(device.target);
        if let Some(image) = cell.first_child().and_downcast::<gtk::Image>() {
            image.set_icon_name(Some(icon));
        }
        if let Some(label) = cell.last_child().and_downcast::<gtk::Label>() {
            label.set_text(&text);
        }
    });
    gtk::ColumnViewColumn::new(Some(&gettext("State")), Some(factory))
}

fn actions_column(
    config: Config,
    handler: DeviceHandler,
    block: ModifyBlock,
) -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    // setup builds the widgets; bind only updates them. The per-row state the
    // buttons need is the bound device, read from the ListItem at click time.
    factory.connect_setup({
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
                return;
            };
            let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            for policy in [
                DevicePolicy::Allow,
                DevicePolicy::Block,
                DevicePolicy::Reject,
            ] {
                buttons.append(&action_button(item, policy, &config, &handler));
            }

            let pending_label = gtk::Label::builder()
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            let cancel = gtk::Button::builder()
                .label(gettext("Cancel"))
                .tooltip_text(gettext(
                    "Stop waiting. The daemon may already have acted; the list will show \
                     what it actually did.",
                ))
                .build();
            cancel.connect_clicked({
                let (item, handler) = (item.downgrade(), handler.clone());
                move |_| {
                    if let Some(device) = item.upgrade().as_ref().and_then(bound_device) {
                        handler(DeviceAction::Cancel(device.id));
                    }
                }
            });
            let pending = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            pending.append(&gtk::Spinner::builder().spinning(true).build());
            pending.append(&pending_label);
            pending.append(&cancel);

            let stack = gtk::Stack::new();
            stack.add_named(&buttons, Some("actions"));
            stack.add_named(&pending, Some("pending"));
            item.set_child(Some(&stack));
        }
    });

    factory.connect_bind(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(device) = bound_device(item) else {
            return;
        };
        let Some(stack) = item.child().and_downcast::<gtk::Stack>() else {
            return;
        };
        if let Some(kind) = device.pending {
            let label = stack
                .child_by_name("pending")
                .and_then(|pending| pending.first_child())
                .and_then(|spinner| spinner.next_sibling())
                .and_downcast::<gtk::Label>();
            if let Some(label) = label {
                label.set_text(&pending_text(kind));
            }
            stack.set_visible_child_name("pending");
        } else {
            stack.set_visible_child_name("actions");
        }
        if let Some(buttons) = stack.child_by_name("actions") {
            let blocked = block.borrow();
            buttons.set_sensitive(blocked.is_none());
            buttons.set_tooltip_text(blocked.as_deref());
        }
    });

    gtk::ColumnViewColumn::new(Some(&gettext("Actions")), Some(factory))
}

/// One of Allow / Block / Reject, with its persistence popover.
fn action_button(
    item: &gtk::ListItem,
    policy: DevicePolicy,
    config: &Config,
    handler: &DeviceHandler,
) -> gtk::MenuButton {
    let (label, verb) = match policy {
        DevicePolicy::Allow => (gettext("Allow"), gettext("Allow this device")),
        DevicePolicy::Block => (gettext("Block"), gettext("Block this device")),
        DevicePolicy::Reject => (gettext("Reject"), gettext("Reject this device")),
    };
    let session = gtk::Button::with_label(&gettext("This session only"));
    session.set_tooltip_text(Some(&gettext("Undone when USBGuard restarts")));
    let permanent = gtk::Button::with_label(&gettext("Permanently"));
    permanent.set_tooltip_text(Some(&gettext("Written to the policy; survives a restart")));

    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .build();
    content.append(
        &gtk::Label::builder()
            .label(&verb)
            .css_classes(["heading"])
            .build(),
    );
    content.append(&session);
    content.append(&permanent);
    let popover = gtk::Popover::builder().child(&content).build();

    // The preselected choice follows the preference, runtime-only by default.
    popover.connect_show({
        let (config, session, permanent) = (config.clone(), session.clone(), permanent.clone());
        move |_| {
            let (preferred, other) = match config.default_persistence() {
                Persistence::RuntimeOnly => (&session, &permanent),
                Persistence::Permanent => (&permanent, &session),
            };
            preferred.add_css_class("suggested-action");
            other.remove_css_class("suggested-action");
            preferred.grab_focus();
        }
    });

    for (button, persistence) in [
        (&session, Persistence::RuntimeOnly),
        (&permanent, Persistence::Permanent),
    ] {
        button.connect_clicked({
            let (item, handler, popover) = (item.downgrade(), handler.clone(), popover.clone());
            move |_| {
                popover.popdown();
                if let Some(device) = item.upgrade().as_ref().and_then(bound_device) {
                    handler(DeviceAction::Apply(device.id, policy, persistence));
                }
            }
        });
    }

    gtk::MenuButton::builder()
        .label(&label)
        .popover(&popover)
        .always_show_arrow(false)
        .build()
}
