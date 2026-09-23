//! The ruleset and the runtime parameters (docs/architecture.md §9.3, §2.5).
//!
//! **The order is semantically load-bearing** — the daemon applies the first
//! matching rule — so it is preserved, numbered, and never re-sorted. That is
//! the difference between this list and the device list, and the page says so.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;

use crate::model::{AttributeName, Parameter, RuleHandle};
use crate::rules::parse_rule;

/// What the page asks the window to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PolicyAction {
    /// Open the new-rule dialog.
    Add,
    /// Remove a rule (after confirmation, by the window).
    Remove(RuleHandle),
    /// Change a runtime parameter from `old` to `new`.
    SetParameter {
        parameter: Parameter,
        old: String,
        new: String,
    },
}

/// Receives page actions.
pub(super) type PolicyHandler = Rc<dyn Fn(PolicyAction)>;

/// Human-readable meaning of a parameter, shown as the row subtitle.
const fn describe(parameter: Parameter) -> &'static str {
    match parameter {
        Parameter::ImplicitPolicyTarget => "What happens to a device that matches no rule",
        Parameter::InsertedDevicePolicy => {
            "What happens to a device plugged in while USBGuard runs"
        }
    }
}

struct ParameterRow {
    parameter: Parameter,
    row: adw::ComboRow,
    /// The value last read from the daemon; `None` until read.
    value: RefCell<Option<String>>,
}

/// The policy page.
#[derive(Clone)]
pub(super) struct PolicyView {
    /// The page's root widget.
    pub(super) root: gtk::Box,
    list_stack: gtk::Stack,
    list: gtk::ListBox,
    error: adw::StatusPage,
    add_button: gtk::Button,
    parameters: Rc<Vec<ParameterRow>>,
    /// True while the rows are being set from daemon values, so that the
    /// selection change is not mistaken for a user choice.
    updating: Rc<Cell<bool>>,
    rules: Rc<RefCell<Vec<RuleHandle>>>,
    handler: PolicyHandler,
    remove_block: Rc<RefCell<Option<String>>>,
}

impl PolicyView {
    pub(super) fn new(handler: PolicyHandler) -> Self {
        let updating = Rc::new(Cell::new(false));

        // Runtime parameters.
        let group = adw::PreferencesGroup::builder()
            .title("Runtime parameters")
            .description(
                "Changes apply immediately and last until USBGuard restarts. Everything else \
                 in usbguard-daemon.conf is startup configuration and cannot be changed from here.",
            )
            .build();
        let parameters: Rc<Vec<ParameterRow>> = Rc::new(
            Parameter::ALL
                .into_iter()
                .map(|parameter| {
                    let row = adw::ComboRow::builder()
                        .title(parameter.name())
                        .subtitle(describe(parameter))
                        .model(&gtk::StringList::new(parameter.allowed_values()))
                        .sensitive(false)
                        .build();
                    group.add(&row);
                    ParameterRow {
                        parameter,
                        row,
                        value: RefCell::new(None),
                    }
                })
                .collect(),
        );
        for (index, p) in parameters.iter().enumerate() {
            p.row.connect_selected_notify({
                let (parameters, updating, handler) =
                    (parameters.clone(), updating.clone(), handler.clone());
                move |row| {
                    if updating.get() {
                        return;
                    }
                    let Some(p) = parameters.get(index) else {
                        return;
                    };
                    let chosen = p
                        .parameter
                        .allowed_values()
                        .get(row.selected() as usize)
                        .map(|v| (*v).to_owned());
                    let old = p.value.borrow().clone();
                    if let (Some(new), Some(old)) = (chosen, old) {
                        if new != old {
                            handler(PolicyAction::SetParameter {
                                parameter: p.parameter,
                                old,
                                new,
                            });
                        }
                    }
                }
            });
        }

        // The ruleset.
        let add_button = gtk::Button::builder()
            .label("Add Rule…")
            .css_classes(["suggested-action"])
            .halign(gtk::Align::End)
            .build();
        add_button.connect_clicked({
            let handler = handler.clone();
            move |_| handler(PolicyAction::Add)
        });
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        heading.append(
            &gtk::Label::builder()
                .label(
                    "Rules are evaluated from top to bottom, and the first rule that matches a \
                     device decides. This list is shown in that order and cannot be re-sorted.",
                )
                .wrap(true)
                .xalign(0.0)
                .hexpand(true)
                .css_classes(["dim-label"])
                .build(),
        );
        heading.append(&add_button);

        let list = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .css_classes(["boxed-list"])
            .build();
        let error = adw::StatusPage::builder()
            .icon_name("dialog-warning-symbolic")
            .title("The ruleset could not be read")
            .build();
        let list_stack = gtk::Stack::builder().vhomogeneous(false).build();
        list_stack.add_named(
            &adw::StatusPage::builder()
                .icon_name("content-loading-symbolic")
                .title("Reading the ruleset…")
                .build(),
            Some("loading"),
        );
        list_stack.add_named(
            &adw::StatusPage::builder()
                .icon_name("view-list-symbolic")
                .title("The ruleset is empty")
                .description("Devices are treated according to the implicit policy target.")
                .build(),
            Some("empty"),
        );
        list_stack.add_named(&error, Some("error"));
        list_stack.add_named(&list, Some("list"));
        list_stack.set_visible_child_name("loading");

        let column = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(18)
            .margin_bottom(18)
            .margin_start(12)
            .margin_end(12)
            .build();
        column.append(&group);
        column.append(&heading);
        column.append(&list_stack);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(
            &gtk::ScrolledWindow::builder()
                .child(
                    &adw::Clamp::builder()
                        .maximum_size(1200)
                        .child(&column)
                        .build(),
                )
                .vexpand(true)
                .build(),
        );

        Self {
            root,
            list_stack,
            list,
            error,
            add_button,
            parameters,
            updating,
            rules: Rc::default(),
            handler,
            remove_block: Rc::default(),
        }
    }

    /// The rules currently shown, in evaluation order.
    pub(super) fn rules(&self) -> Vec<RuleHandle> {
        self.rules.borrow().clone()
    }

    pub(super) fn set_rules(&self, rules: Vec<RuleHandle>) {
        self.list.remove_all();
        for rule in &rules {
            self.list.append(&self.row(rule));
        }
        self.list_stack
            .set_visible_child_name(if rules.is_empty() { "empty" } else { "list" });
        *self.rules.borrow_mut() = rules;
    }

    pub(super) fn set_error(&self, message: &str) {
        self.error.set_description(Some(message));
        self.list_stack.set_visible_child_name("error");
    }

    /// Shows a parameter value read from, or reported by, the daemon.
    /// Returns the previous value, if one was known.
    pub(super) fn set_parameter(&self, parameter: Parameter, value: &str) -> Option<String> {
        let p = self.parameters.iter().find(|p| p.parameter == parameter)?;
        let index = parameter.allowed_values().iter().position(|v| *v == value);
        self.updating.set(true);
        if let Some(index) = index.and_then(|i| u32::try_from(i).ok()) {
            p.row.set_selected(index);
        }
        p.row.set_sensitive(true);
        self.updating.set(false);
        p.value.replace(Some(value.to_owned()))
    }

    /// Puts a parameter row back on its last known value — after a refused
    /// or cancelled change.
    pub(super) fn revert_parameter(&self, parameter: Parameter) {
        let known = self
            .parameters
            .iter()
            .find(|p| p.parameter == parameter)
            .and_then(|p| p.value.borrow().clone());
        if let Some(value) = known {
            self.set_parameter(parameter, &value);
        }
    }

    /// Makes rule and parameter changes insensitive, with the reason as
    /// tooltip, when the daemon has refused them (§3.5).
    pub(super) fn set_blocks(&self, rules: Option<&str>, parameters: Option<&str>) {
        self.add_button.set_sensitive(rules.is_none());
        self.add_button.set_tooltip_text(rules);
        *self.remove_block.borrow_mut() = rules.map(str::to_owned);
        let rules = self.rules();
        self.set_rules(rules);
        for p in self.parameters.iter() {
            if p.value.borrow().is_some() {
                p.row.set_sensitive(parameters.is_none());
            }
            p.row.set_tooltip_text(parameters);
        }
    }

    fn row(&self, rule: &RuleHandle) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(&rule.text)
            // Rule text contains quotes and may contain `<`: never markup.
            .use_markup(false)
            .title_selectable(true)
            .css_classes(["monospace"])
            .build();
        let label = parse_rule(&rule.text)
            .ok()
            .and_then(|r| r.string_value(AttributeName::Label).cloned());
        if let Some(label) = label {
            row.set_subtitle(&format!("label: {}", label.to_string_lossy()));
        }
        row.add_prefix(
            &gtk::Label::builder()
                .label(format!("{}.", rule.position + 1))
                .width_chars(4)
                .xalign(1.0)
                .css_classes(["numeric", "dim-label"])
                .build(),
        );
        row.set_tooltip_text(Some(&format!("Daemon rule id: {}", rule.id)));

        let remove = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let block = self.remove_block.borrow();
        remove.set_sensitive(block.is_none());
        remove.set_tooltip_text(Some(block.as_deref().unwrap_or("Remove this rule")));
        remove.update_property(&[gtk::accessible::Property::Label("Remove rule")]);
        remove.connect_clicked({
            let (handler, rule) = (self.handler.clone(), rule.clone());
            move |_| handler(PolicyAction::Remove(rule.clone()))
        });
        row.add_suffix(&remove);
        row
    }
}
