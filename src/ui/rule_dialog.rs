//! The new-rule dialog (docs/architecture.md §9.4).
//!
//! Three properties it must have:
//!
//! 1. A live preview of the canonical text that will be sent.
//! 2. Local validation before sending: an invalid rule never reaches the
//!    daemon and never causes a Polkit prompt.
//! 3. An explicit position — "at the end" or "after «…»" — because the API
//!    takes a parent rule, not an index (§2.4.6).
//!
//! Plus the persistence choice, hidden where the daemon's `appendRule` does
//! not accept it (API older than 1.1.0), and a raw-text mode with the same
//! validation.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::dbus::diagnostics::{self, ApiLevel};
use crate::model::{
    AttributeName, ParseError, Persistence, Rule, RuleHandle, RuleTarget, SetOperator,
};
use crate::rules::builder::{FieldInput, build_rule, value_hint};
use crate::rules::{parse_rule, render_checked};
use crate::runtime::runtime;

/// What the user asked to add.
#[derive(Debug, Clone)]
pub(super) struct NewRule {
    pub(super) rule: Rule,
    /// Insert after this rule; `None` means at the end.
    pub(super) after: Option<RuleHandle>,
    pub(super) persistence: Persistence,
}

const TARGETS: [RuleTarget; 3] = [RuleTarget::Allow, RuleTarget::Block, RuleTarget::Reject];
/// The operator dropdown: index 0 is "no operator".
const OPERATOR_LABELS: [&str; 7] = [
    "(single value)",
    "all-of",
    "one-of",
    "none-of",
    "equals",
    "equals-ordered",
    "match-all",
];

/// One attribute row of the guided form.
struct FieldRow {
    root: gtk::Box,
    name: gtk::DropDown,
    operator: gtk::DropDown,
    entry: gtk::Entry,
}

impl FieldRow {
    fn input(&self) -> Option<FieldInput> {
        let name = *AttributeName::ALL.get(self.name.selected() as usize)?;
        let operator = match self.operator.selected() {
            0 => None,
            i => SetOperator::ALL.get(i as usize - 1).copied(),
        };
        Some(FieldInput {
            name,
            operator,
            text: self.entry.text().to_string(),
        })
    }
}

struct Form {
    dialog: adw::Dialog,
    add: gtk::Button,
    mode: gtk::Stack,
    target: adw::ComboRow,
    fields_group: adw::PreferencesGroup,
    fields_box: gtk::ListBox,
    fields: RefCell<Vec<FieldRow>>,
    text: gtk::Entry,
    preview: gtk::Label,
    message: gtk::Label,
    position: adw::ComboRow,
    persistent: adw::SwitchRow,
    rules: Vec<RuleHandle>,
    current: RefCell<Option<Rule>>,
}

/// Opens the dialog. `on_submit` receives the validated rule.
pub(super) fn present(
    parent: &impl IsA<gtk::Widget>,
    rules: Vec<RuleHandle>,
    default_persistence: Persistence,
    on_submit: impl Fn(NewRule) + 'static,
) {
    let form = Rc::new(build(rules, default_persistence));

    form.add.connect_clicked({
        let form = form.clone();
        move |_| {
            let Some(rule) = form.current.borrow().clone() else {
                return;
            };
            let after = match form.position.selected() {
                0 => None,
                i => form.rules.get(i as usize - 1).cloned(),
            };
            let persistence = if form.persistent.is_active() {
                Persistence::Permanent
            } else {
                Persistence::RuntimeOnly
            };
            form.dialog.close();
            on_submit(NewRule {
                rule,
                after,
                persistence,
            });
        }
    });

    let refresh = {
        let form = form.clone();
        move || recompute(&form)
    };
    let refresh: Rc<dyn Fn()> = Rc::new(refresh);
    form.target.connect_selected_notify({
        let refresh = refresh.clone();
        move |_| refresh()
    });
    form.text.connect_changed({
        let refresh = refresh.clone();
        move |_| refresh()
    });
    form.mode.connect_visible_child_name_notify({
        let (form, refresh) = (form.clone(), refresh.clone());
        move |_| {
            // Carry the guided rule over into the text mode as a start.
            if form.mode.visible_child_name().as_deref() == Some("text") {
                if let Some(rule) = form.current.borrow().as_ref() {
                    form.text.set_text(&rule.to_string());
                }
            }
            refresh();
        }
    });

    let add_field = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add an attribute")
        .css_classes(["flat"])
        .build();
    add_field.connect_clicked({
        let (form, refresh) = (form.clone(), refresh.clone());
        move |_| add_field_row(&form, &refresh)
    });
    form.fields_group.set_header_suffix(Some(&add_field));

    // The persistence switch exists only where appendRule has `temporary`.
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let level = match zbus::Connection::system().await {
            Ok(connection) => diagnostics::api_level(&connection).await,
            Err(_) => None,
        };
        let _ = tx.send(level).await;
    });
    glib::spawn_future_local({
        let form = form.clone();
        async move {
            if let Ok(level) = rx.recv().await {
                form.persistent
                    .set_visible(level != Some(ApiLevel::Before1_1));
            }
        }
    });

    recompute(&form);
    form.dialog.present(Some(parent));
}

fn build(rules: Vec<RuleHandle>, default_persistence: Persistence) -> Form {
    let cancel = gtk::Button::with_label("Cancel");
    let add = gtk::Button::builder()
        .label("Add")
        .css_classes(["suggested-action"])
        .sensitive(false)
        .build();
    let header = adw::HeaderBar::builder()
        .show_start_title_buttons(false)
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel);
    header.pack_end(&add);

    // Guided mode.
    let target = adw::ComboRow::builder()
        .title("Target")
        .model(&gtk::StringList::new(&["allow", "block", "reject"]))
        .build();
    let target_group = adw::PreferencesGroup::new();
    target_group.add(&target);
    let fields_box = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    let fields_group = adw::PreferencesGroup::builder()
        .title("Attributes")
        .description(
            "A device matches when every attribute matches. No attribute matches every device.",
        )
        .build();
    fields_group.add(&fields_box);
    let guided = gtk::Box::new(gtk::Orientation::Vertical, 18);
    guided.append(&target_group);
    guided.append(&fields_group);

    // Text mode.
    let text = gtk::Entry::builder()
        .placeholder_text("allow id 1234:5678 with-interface 08:*:*")
        .css_classes(["monospace"])
        .build();
    let text_group = adw::PreferencesGroup::builder()
        .title("Rule text")
        .description(
            "The usbguard-rules.conf(5) syntax. The target must be allow, block, or reject.",
        )
        .build();
    text_group.add(&text);

    let mode = gtk::Stack::builder().vhomogeneous(false).build();
    mode.add_titled(&guided, Some("guided"), "Guided");
    mode.add_titled(&text_group, Some("text"), "Text");
    let switcher = gtk::StackSwitcher::builder()
        .stack(&mode)
        .halign(gtk::Align::Center)
        .build();

    // Preview.
    let preview = gtk::Label::builder()
        .selectable(true)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .css_classes(["monospace"])
        .build();
    let message = gtk::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .css_classes(["error", "monospace"])
        .visible(false)
        .build();
    let preview_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    preview_box.append(&preview);
    preview_box.append(&message);
    let preview_frame = gtk::Frame::builder().child(&preview_box).build();
    let preview_group = adw::PreferencesGroup::builder()
        .title("Will be sent as")
        .build();
    preview_group.add(&preview_frame);

    // Placement.
    let mut positions: Vec<String> = vec!["At the end".to_owned()];
    positions.extend(rules.iter().map(|r| {
        let mut short: String = r.text.chars().take(48).collect();
        if r.text.chars().count() > 48 {
            short.push('…');
        }
        format!("After {}. {short}", r.position + 1)
    }));
    let position_refs: Vec<&str> = positions.iter().map(String::as_str).collect();
    let position = adw::ComboRow::builder()
        .title("Position")
        .subtitle("The first matching rule decides")
        .model(&gtk::StringList::new(&position_refs))
        .build();
    let persistent = adw::SwitchRow::builder()
        .title("Keep after USBGuard restarts")
        .subtitle("Off: the rule lasts until the daemon restarts")
        .active(default_persistence == Persistence::Permanent)
        .build();
    let placement = adw::PreferencesGroup::builder().title("Placement").build();
    placement.add(&position);
    placement.add(&persistent);

    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(12)
        .margin_bottom(18)
        .margin_start(12)
        .margin_end(12)
        .build();
    column.append(&switcher);
    column.append(&mode);
    column.append(&preview_group);
    column.append(&placement);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .child(&adw::Clamp::builder().child(&column).build())
            .propagate_natural_height(true)
            .build(),
    ));

    let dialog = adw::Dialog::builder()
        .title("New Rule")
        .content_width(640)
        .child(&toolbar)
        .build();
    cancel.connect_clicked({
        let dialog = dialog.clone();
        move |_| {
            dialog.close();
        }
    });

    Form {
        dialog,
        add,
        mode,
        target,
        fields_group,
        fields_box,
        fields: RefCell::new(Vec::new()),
        text,
        preview,
        message,
        position,
        persistent,
        rules,
        current: RefCell::new(None),
    }
}

fn add_field_row(form: &Rc<Form>, refresh: &Rc<dyn Fn()>) {
    let names: Vec<&str> = AttributeName::ALL.iter().map(|a| a.keyword()).collect();
    let name = gtk::DropDown::from_strings(&names);
    let operator = gtk::DropDown::from_strings(&OPERATOR_LABELS);
    operator.set_tooltip_text(Some("How several values are compared with the device's"));
    let entry = gtk::Entry::builder()
        .hexpand(true)
        .placeholder_text(value_hint(AttributeName::Id))
        .build();
    let remove = gtk::Button::builder()
        .icon_name("list-remove-symbolic")
        .tooltip_text("Remove this attribute")
        .css_classes(["flat"])
        .build();
    let root = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(6)
        .margin_end(6)
        .build();
    root.append(&name);
    root.append(&operator);
    root.append(&entry);
    root.append(&remove);

    name.connect_selected_notify({
        let (entry, refresh) = (entry.clone(), refresh.clone());
        move |dd| {
            if let Some(a) = AttributeName::ALL.get(dd.selected() as usize) {
                entry.set_placeholder_text(Some(value_hint(*a)));
            }
            refresh();
        }
    });
    operator.connect_selected_notify({
        let refresh = refresh.clone();
        move |_| refresh()
    });
    entry.connect_changed({
        let refresh = refresh.clone();
        move |_| refresh()
    });
    remove.connect_clicked({
        let (form, root, refresh) = (form.clone(), root.clone(), refresh.clone());
        move |_| {
            form.fields.borrow_mut().retain(|f| f.root != root);
            if let Some(row) = root.parent() {
                form.fields_box.remove(&row);
            }
            refresh();
        }
    });

    form.fields_box.append(&root);
    form.fields.borrow_mut().push(FieldRow {
        root,
        name,
        operator,
        entry: entry.clone(),
    });
    entry.grab_focus();
    refresh();
}

/// Rebuilds the rule from whichever mode is visible, and updates the preview,
/// the error message, and the Add button.
fn recompute(form: &Form) {
    let guided = form.mode.visible_child_name().as_deref() != Some("text");
    for f in form.fields.borrow().iter() {
        f.entry.remove_css_class("error");
    }
    let result: Result<Rule, String> = if guided {
        let target = TARGETS
            .get(form.target.selected() as usize)
            .copied()
            .unwrap_or(RuleTarget::Block);
        let inputs: Vec<FieldInput> = form
            .fields
            .borrow()
            .iter()
            .filter_map(FieldRow::input)
            .collect();
        build_rule(target, &inputs).map_err(|e| {
            if let Some(f) = form.fields.borrow().get(e.row) {
                f.entry.add_css_class("error");
            }
            e.message
        })
    } else {
        let text = form.text.text();
        parse_rule(&text).map_err(|e| caret_message(&text, e))
    };

    // Round-trip check before anything is sent (§7.2).
    let result = result.and_then(|rule| {
        render_checked(&rule)
            .map(|text| (rule, text))
            .map_err(|e| format!("internal error: the rule does not survive rendering ({e})"))
    });

    match result {
        Ok((rule, text)) => {
            form.preview.set_text(&text);
            form.message.set_visible(false);
            form.add.set_sensitive(true);
            *form.current.borrow_mut() = Some(rule);
        }
        Err(message) => {
            form.message.set_text(&message);
            form.message.set_visible(true);
            form.add.set_sensitive(false);
            *form.current.borrow_mut() = None;
        }
    }
}

/// "error at character N" plus the input with a caret under the offending
/// position.
fn caret_message(text: &str, err: ParseError) -> String {
    let column = text
        .get(..err.offset.min(text.len()))
        .map_or(err.offset, |prefix| prefix.chars().count());
    format!(
        "{} (character {}):\n{text}\n{}^",
        err.kind.describe(),
        column + 1,
        " ".repeat(column)
    )
}
