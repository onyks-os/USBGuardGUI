//! The window shell (docs/architecture.md §9.1), the one function that turns a
//! [`UiEvent`] into widget changes ([`MainWindow::apply`]), and the lifecycle
//! of every user operation: start, pending notices, cancel, outcome.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::{gio, glib};

use gettextrs::{gettext, ngettext};

use super::APP_NAME;
use super::config::Config;
use super::device_view::{DeviceAction, DeviceView, ModifyBlock};
use super::i18n::{self, fill};
use super::notify::Notifier;
use super::policy_view::{PolicyAction, PolicyView};
use super::rule_dialog::{self, NewRule};
use super::tray::{self, TrayCommand, TrayResult, UsbguardTray};
use super::{diagnostic_dialog, preferences, reload_devices, reload_parameters, reload_rules};
use crate::dbus::Client;
use crate::dbus::commands::{self, NO_AGENT_HINT_AFTER, PendingOperation, WAITING_NOTICE_AFTER};
use crate::device_store::DeviceStore;
use crate::model::{
    AccessState, AppError, Device, DeviceId, DevicePolicy, OperationId, OperationOutcome,
    Parameter, PendingKind, Persistence, RemoveOutcome, RuleHandle, Target, UiEvent,
};

/// The connection indicator in the header bar (§9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Indicator {
    Checking,
    Connected,
    ReadOnly,
    Denied,
    Disconnected,
}

impl Indicator {
    fn presentation(self) -> (&'static str, String) {
        match self {
            Self::Checking => ("content-loading-symbolic", gettext("Checking…")),
            Self::Connected => ("emblem-ok-symbolic", gettext("Connected")),
            // Translators: the connection state when changes are not permitted.
            Self::ReadOnly => ("changes-prevent-symbolic", gettext("Read-only")),
            Self::Denied => ("dialog-warning-symbolic", gettext("Access denied")),
            Self::Disconnected => ("network-offline-symbolic", gettext("Disconnected")),
        }
    }
}

/// What an operation in flight is doing, to interpret its outcome.
#[derive(Debug, Clone)]
enum OpKind {
    Device {
        id: DeviceId,
        policy: DevicePolicy,
        persistence: Persistence,
        name: String,
    },
    RemoveRule,
    AppendRule,
    SetParameter {
        parameter: Parameter,
        new: String,
    },
}

/// Which write a denial disables (§3.5).
#[derive(Debug, Clone, Copy)]
enum Write {
    Devices,
    Rules,
    Parameters,
}

#[derive(Default)]
struct Denials {
    devices: Option<String>,
    rules: Option<String>,
    parameters: Option<String>,
}

struct State {
    devices: DeviceStore,
    /// The ruleset may be out of date; re-read it when the page is shown.
    rules_stale: bool,
    connected: bool,
    access: Option<AccessState>,
    denials: Denials,
    ops: HashMap<OperationId, (PendingOperation, OpKind)>,
}

struct Inner {
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    banner: adw::Banner,
    indicator: adw::ButtonContent,
    stack: adw::ViewStack,
    devices: DeviceView,
    policy: PolicyView,
    device_block: ModifyBlock,
    config: Config,
    state: RefCell<State>,
    ui_tx: async_channel::Sender<UiEvent>,
    app: adw::Application,
    notifier: Notifier,
    /// Keeps the application alive while the window is hidden.
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
    tray: RefCell<Option<ksni::Handle<UsbguardTray>>>,
    tray_commands: async_channel::Sender<TrayCommand>,
}

/// The main window. Cheap to clone: it is a reference to shared state.
#[derive(Clone)]
pub(super) struct MainWindow(Rc<Inner>);

const POLICY_PAGE: &str = "policy";

impl MainWindow {
    pub(super) fn new(
        app: &adw::Application,
        config: Config,
        ui_tx: async_channel::Sender<UiEvent>,
        tray_commands: async_channel::Sender<TrayCommand>,
    ) -> Self {
        // The views call back into the window, which does not exist yet:
        // they get a slot that is filled once it does.
        let slot: Rc<OnceCell<Weak<Inner>>> = Rc::default();
        let with_window = |slot: &Rc<OnceCell<Weak<Inner>>>| {
            let slot = slot.clone();
            move || slot.get().and_then(Weak::upgrade).map(MainWindow)
        };

        let device_block: ModifyBlock = Rc::default();
        let devices = DeviceView::new(
            &config,
            Rc::new({
                let window = with_window(&slot);
                move |action| {
                    if let Some(w) = window() {
                        w.on_device_action(action);
                    }
                }
            }),
            device_block.clone(),
        );
        let policy = PolicyView::new(Rc::new({
            let window = with_window(&slot);
            move |action| {
                if let Some(w) = window() {
                    w.on_policy_action(action);
                }
            }
        }));

        let stack = adw::ViewStack::new();
        stack.add_titled_with_icon(
            &devices.root,
            Some("devices"),
            &gettext("Devices"),
            "drive-removable-media-symbolic",
        );
        stack.add_titled_with_icon(
            &policy.root,
            Some(POLICY_PAGE),
            // Translators: the page listing USBGuard's rules.
            &gettext("Policy"),
            "view-list-symbolic",
        );

        let indicator = adw::ButtonContent::new();
        let status_button = gtk::Button::builder()
            .child(&indicator)
            .tooltip_text(gettext("Show USBGuard access diagnostics"))
            .build();

        let menu = gio::Menu::new();
        menu.append(Some(&gettext("_Preferences")), Some("win.preferences"));
        menu.append(
            Some(&fill(&gettext("_About {app}"), &[("app", APP_NAME)])),
            Some("win.about"),
        );
        menu.append(Some(&gettext("_Quit")), Some("app.quit"));
        let menu_button = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .tooltip_text(gettext("Main menu"))
            .primary(true)
            .build();

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(
            &adw::ViewSwitcher::builder()
                .stack(&stack)
                .policy(adw::ViewSwitcherPolicy::Wide)
                .build(),
        ));
        header.pack_end(&menu_button);
        header.pack_end(&status_button);

        let banner = adw::Banner::builder()
            .revealed(false)
            .use_markup(false)
            .build();
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&stack));

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.add_top_bar(&banner);
        toolbar.set_content(Some(&toasts));

        let (width, height) = config.window_size();
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title(APP_NAME)
            .default_width(width)
            .default_height(height)
            .maximized(config.window_maximized())
            .content(&toolbar)
            .build();

        let inner = Rc::new(Inner {
            window,
            toasts,
            banner,
            indicator,
            stack,
            devices,
            policy,
            device_block,
            config,
            state: RefCell::new(State {
                devices: DeviceStore::new(),
                rules_stale: true,
                connected: false,
                access: None,
                denials: Denials::default(),
                ops: HashMap::new(),
            }),
            ui_tx,
            app: app.clone(),
            notifier: Notifier::new(app),
            hold: RefCell::default(),
            tray: RefCell::default(),
            tray_commands,
        });
        let _ = slot.set(Rc::downgrade(&inner));
        let this = Self(inner);
        this.set_indicator(Indicator::Checking);
        this.connect_signals(&status_button, app);
        this
    }

    fn connect_signals(&self, status_button: &gtk::Button, app: &adw::Application) {
        let inner = &self.0;
        status_button.connect_clicked({
            let window = inner.window.clone();
            move |_| diagnostic_dialog::present(&window)
        });
        inner.banner.connect_button_clicked({
            let window = inner.window.clone();
            move |_| diagnostic_dialog::present(&window)
        });
        inner.stack.connect_visible_child_name_notify({
            let this = self.clone();
            move |_| this.reload_policy_if_needed()
        });
        inner.window.connect_close_request({
            let this = self.clone();
            move |w| {
                let (width, height) = w.default_size();
                this.0.config.save_window(width, height, w.is_maximized());
                if this.0.config.run_in_background() {
                    this.hide_to_background();
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
        });
        if let Some(settings) = inner.config.settings() {
            settings.connect_changed(Some("run-in-background"), {
                let this = self.clone();
                move |_, _| this.sync_tray()
            });
        }
        self.sync_tray();

        let preferences = gio::ActionEntry::builder("preferences")
            .activate({
                let config = inner.config.clone();
                move |w: &adw::ApplicationWindow, _, _| preferences::present_preferences(w, &config)
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|w: &adw::ApplicationWindow, _, _| preferences::present_about(w))
            .build();
        inner.window.add_action_entries([preferences, about]);
        app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    }

    pub(super) fn present(&self) {
        self.0.hold.borrow_mut().take();
        self.0.notifier.withdraw();
        self.0.window.present();
    }

    /// Opens the window on the device list — the notification's default action.
    pub(super) fn show_devices(&self) {
        self.0.stack.set_visible_child_name("devices");
        self.present();
    }

    /// Starts without a window (`--background`), kept alive by a hold.
    pub(super) fn start_hidden(&self) {
        *self.0.hold.borrow_mut() = Some(self.0.app.hold());
    }

    fn hide_to_background(&self) {
        *self.0.hold.borrow_mut() = Some(self.0.app.hold());
        self.0.window.set_visible(false);
        if self.0.config.take_background_notice() {
            let has_tray = self.0.tray.borrow().is_some();
            let n = gio::Notification::new(&fill(
                &gettext("{app} is still running"),
                &[("app", APP_NAME)],
            ));
            let body = if has_tray {
                gettext(
                    "It will announce newly inserted devices. Use the status icon to reopen it \
                     or quit.",
                )
            } else {
                fill(
                    &gettext(
                        "It will announce newly inserted devices. This desktop has no status \
                         icon area: open {app} again from the application menu to show the \
                         window, and quit from its main menu.",
                    ),
                    &[("app", APP_NAME)],
                )
            };
            n.set_body(Some(&body));
            self.0.app.send_notification(Some("background"), &n);
        }
    }

    /// Starts or stops the status icon to match the `run-in-background`
    /// setting.
    fn sync_tray(&self) {
        let wanted = self.0.config.run_in_background();
        let running = self.0.tray.borrow().is_some();
        if wanted && !running {
            let (reply, answer) = async_channel::bounded::<TrayResult>(1);
            tray::start(self.0.tray_commands.clone(), reply);
            let weak = Rc::downgrade(&self.0);
            glib::spawn_future_local(async move {
                let Ok(result) = answer.recv().await else {
                    return;
                };
                let Some(inner) = weak.upgrade() else { return };
                match result {
                    Ok(handle) => {
                        *inner.tray.borrow_mut() = Some(handle);
                        MainWindow(inner).refresh_devices();
                    }
                    // No StatusNotifierWatcher: background mode without an
                    // icon, explained once by hide_to_background.
                    Err(reason) => tracing::info!("no status icon: {reason}"),
                }
            });
        } else if !wanted {
            if let Some(handle) = self.0.tray.borrow_mut().take() {
                tray::stop(&handle);
            }
        }
    }

    /// "Allow for this session" from a notification. Only if the device with
    /// that id is still the one the notification described.
    pub(super) fn allow_from_notification(&self, id: DeviceId) {
        let device = self
            .0
            .state
            .borrow()
            .devices
            .sorted()
            .into_iter()
            .find(|d| d.id == id);
        match device {
            Some(device) if self.0.notifier.still_offered(&device) => {
                self.start_device_op(id, DevicePolicy::Allow, Persistence::RuntimeOnly);
            }
            _ => {
                self.show_devices();
                self.toast(&gettext(
                    "That device is no longer connected. Nothing was changed.",
                ));
            }
        }
    }

    /// Runs `f` once, the first time the window is on screen.
    pub(super) fn connect_first_map(&self, f: impl FnOnce() + 'static) {
        let f = RefCell::new(Some(f));
        self.0.window.connect_map(move |_| {
            if let Some(f) = f.borrow_mut().take() {
                f();
            }
        });
    }

    // -----------------------------------------------------------------------
    // Presentation helpers
    // -----------------------------------------------------------------------

    fn set_indicator(&self, indicator: Indicator) {
        let (icon, label) = indicator.presentation();
        self.0.indicator.set_icon_name(icon);
        self.0.indicator.set_label(&label);
    }

    /// The indicator for a working connection: read-only when both device and
    /// rule changes have been refused — a legitimate configuration, not a
    /// degraded one (§3.3).
    fn connected_indicator(&self) -> Indicator {
        let state = self.0.state.borrow();
        if state.denials.devices.is_some() && state.denials.rules.is_some() {
            Indicator::ReadOnly
        } else {
            Indicator::Connected
        }
    }

    fn show_banner(&self, title: &str, button: Option<&str>) {
        self.0.banner.set_title(title);
        self.0.banner.set_button_label(button);
        self.0.banner.set_revealed(true);
    }

    fn toast(&self, text: &str) {
        // Plain text: the daemon's messages may contain `<` or `&`.
        let toast = adw::Toast::builder()
            .title(text)
            .use_markup(false)
            .timeout(6)
            .build();
        self.0.toasts.add_toast(toast);
    }

    fn refresh_devices(&self) {
        let sorted = self.0.state.borrow().devices.sorted();
        if let Some(handle) = self.0.tray.borrow().as_ref() {
            let blocked = sorted.iter().filter(|d| d.target != Target::Allow).count();
            tray::update(handle, sorted.len(), blocked);
        }
        self.0.devices.set_devices(sorted);
    }

    /// Announces devices that appeared since `before` and are not authorized
    /// (§9.5): a notification, or a toast when the window is in front.
    fn announce_new(&self, before: &[DeviceId]) {
        if !self.0.config.notify_inserted() {
            return;
        }
        let new: Vec<Device> = self
            .0
            .state
            .borrow()
            .devices
            .sorted()
            .into_iter()
            .filter(|d| !before.contains(&d.id) && d.target != Target::Allow)
            .collect();
        if new.is_empty() {
            return;
        }
        if self.0.window.is_visible() && self.0.window.is_active() {
            let text = if let [only] = new.as_slice() {
                match only
                    .attrs
                    .name
                    .as_deref()
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                {
                    Some(name) => fill(
                        &gettext("“{device}” was plugged in and is not authorized"),
                        &[("device", name)],
                    ),
                    None => gettext("A device was plugged in and is not authorized"),
                }
            } else {
                let count = u32::try_from(new.len()).unwrap_or(u32::MAX);
                fill(
                    &ngettext(
                        "{count} device was plugged in and is not authorized",
                        "{count} devices were plugged in and are not authorized",
                        count,
                    ),
                    &[("count", &count.to_string())],
                )
            };
            self.toast(&text);
        } else {
            self.0.notifier.announce(&new);
        }
    }

    fn known_ids(&self) -> Vec<DeviceId> {
        self.0
            .state
            .borrow()
            .devices
            .sorted()
            .iter()
            .map(|d| d.id)
            .collect()
    }

    fn reload_policy_if_needed(&self) {
        let visible = self.0.stack.visible_child_name().as_deref() == Some(POLICY_PAGE);
        let mut state = self.0.state.borrow_mut();
        if visible && state.rules_stale && state.connected {
            // Opportunistic: also covers CLI edits that produced no signal (§5.6).
            state.rules_stale = false;
            reload_rules(self.0.ui_tx.clone());
            reload_parameters(self.0.ui_tx.clone());
        }
    }

    /// Asks a yes/no question; `on_answer` gets `true` for the affirmative.
    fn confirm(
        &self,
        heading: &str,
        body: &str,
        yes: &str,
        destructive: bool,
        on_answer: impl Fn(bool) + 'static,
    ) {
        let dialog = adw::AlertDialog::new(Some(heading), Some(body));
        dialog.add_responses(&[("cancel", "Cancel"), ("yes", yes)]);
        dialog.set_response_appearance(
            "yes",
            if destructive {
                adw::ResponseAppearance::Destructive
            } else {
                adw::ResponseAppearance::Suggested
            },
        );
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");
        dialog.connect_response(None, move |_, response| on_answer(response == "yes"));
        dialog.present(Some(&self.0.window));
    }

    // -----------------------------------------------------------------------
    // Capabilities (§3.5)
    // -----------------------------------------------------------------------

    fn deny_write(&self, write: Write, reason: &str) {
        let reason = fill(
            &gettext("Not permitted: {reason}. See the diagnostics for the remedy."),
            &[("reason", reason)],
        );
        {
            let mut state = self.0.state.borrow_mut();
            let slot = match write {
                Write::Devices => &mut state.denials.devices,
                Write::Rules => &mut state.denials.rules,
                Write::Parameters => &mut state.denials.parameters,
            };
            *slot = Some(reason);
        }
        self.apply_denials();
    }

    fn apply_denials(&self) {
        let (devices, rules, parameters) = {
            let state = self.0.state.borrow();
            (
                state.denials.devices.clone(),
                state.denials.rules.clone(),
                state.denials.parameters.clone(),
            )
        };
        *self.0.device_block.borrow_mut() = devices;
        self.refresh_devices();
        self.0
            .policy
            .set_blocks(rules.as_deref(), parameters.as_deref());
        if self.0.state.borrow().connected && !self.access_problem() {
            self.set_indicator(self.connected_indicator());
        }
    }

    // -----------------------------------------------------------------------
    // Operations (§5.7)
    // -----------------------------------------------------------------------

    fn start<F>(&self, label: String, kind: OpKind, work: F) -> OperationId
    where
        F: Future<Output = Result<OperationOutcome, AppError>> + Send + 'static,
    {
        let op = commands::spawn(label, self.0.ui_tx.clone(), work);
        let id = op.id;
        self.0.state.borrow_mut().ops.insert(id, (op, kind));
        id
    }

    fn on_device_action(&self, action: DeviceAction) {
        match action {
            DeviceAction::Apply(id, DevicePolicy::Reject, persistence) => {
                let name = self.device_name(id);
                let this = self.clone();
                self.confirm(
                    &fill(&gettext("Reject “{device}”?"), &[("device", &name)]),
                    &gettext(
                        "Rejecting removes the device from the system. It can only be used \
                         again after it is physically unplugged and plugged back in.",
                    ),
                    &gettext("Reject"),
                    true,
                    move |yes| {
                        if yes {
                            this.start_device_op(id, DevicePolicy::Reject, persistence);
                        }
                    },
                );
            }
            DeviceAction::Apply(id, policy, persistence) => {
                self.start_device_op(id, policy, persistence);
            }
            DeviceAction::Cancel(id) => self.cancel_device_op(id),
        }
    }

    fn device_name(&self, id: DeviceId) -> String {
        self.0
            .state
            .borrow()
            .devices
            .sorted()
            .into_iter()
            .find(|d| d.id == id)
            .and_then(|d| d.attrs.name)
            .map(|n| n.trim().to_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| fill(&gettext("device {id}"), &[("id", &id.to_string())]))
    }

    fn start_device_op(&self, id: DeviceId, policy: DevicePolicy, persistence: Persistence) {
        let name = self.device_name(id);
        let op = self.start(
            format!("{policy:?} {name}"),
            OpKind::Device {
                id,
                policy,
                persistence,
                name,
            },
            async move {
                let client = Client::connect_system().await?;
                let rule = client.apply_device_policy(id, policy, persistence).await?;
                Ok(OperationOutcome::DevicePolicyApplied { rule })
            },
        );
        self.0
            .state
            .borrow_mut()
            .devices
            .set_pending(id, Some(PendingKind::Started));
        self.refresh_devices();

        // The notices of §5.7. The operation itself is never auto-cancelled:
        // a slow agent and a person fetching a password look the same.
        for (after, stage) in [
            (WAITING_NOTICE_AFTER, PendingKind::WaitingForAuthentication),
            (NO_AGENT_HINT_AFTER, PendingKind::PossiblyNoAgent),
        ] {
            let weak = Rc::downgrade(&self.0);
            glib::timeout_add_local_once(after, move || {
                let Some(inner) = weak.upgrade() else { return };
                let this = MainWindow(inner);
                let still_running = this.0.state.borrow().ops.contains_key(&op);
                if still_running {
                    this.0
                        .state
                        .borrow_mut()
                        .devices
                        .set_pending(id, Some(stage));
                    this.refresh_devices();
                }
            });
        }
    }

    fn cancel_device_op(&self, device: DeviceId) {
        let op = {
            let mut state = self.0.state.borrow_mut();
            let key = state.ops.iter().find_map(|(key, (_, kind))| match kind {
                OpKind::Device { id, .. } if *id == device => Some(*key),
                _ => None,
            });
            key.and_then(|k| state.ops.remove(&k))
        };
        let Some((op, _)) = op else { return };
        op.cancel();
        self.0.state.borrow_mut().devices.set_pending(device, None);
        self.refresh_devices();
        self.toast(&gettext(
            "Stopped waiting. USBGuard may already have acted; the list shows what it actually did.",
        ));
        // Cancellation is client-side only: re-read rather than presume.
        reload_devices(self.0.ui_tx.clone());
    }

    fn on_policy_action(&self, action: PolicyAction) {
        match action {
            PolicyAction::Add => {
                let this = self.clone();
                rule_dialog::present(
                    &self.0.window,
                    self.0.policy.rules(),
                    self.0.config.default_persistence(),
                    move |new| this.start_append(new),
                );
            }
            PolicyAction::Remove(handle) => {
                let this = self.clone();
                let (heading, body) = (
                    fill(
                        &gettext("Remove rule {position}?"),
                        &[("position", &(handle.position + 1).to_string())],
                    ),
                    handle.text.clone(),
                );
                self.confirm(&heading, &body, &gettext("Remove"), true, move |yes| {
                    if yes {
                        this.start_remove(handle.clone(), false);
                    }
                });
            }
            PolicyAction::SetParameter {
                parameter,
                old,
                new,
            } => {
                let this = self.clone();
                self.confirm(
                    &fill(
                        &gettext("Change {parameter}?"),
                        &[("parameter", parameter.name())],
                    ),
                    &fill(
                        &gettext(
                            "From “{old}” to “{new}”. The change applies immediately and lasts \
                             until USBGuard restarts.",
                        ),
                        &[("old", &old), ("new", &new)],
                    ),
                    &gettext("Change"),
                    false,
                    move |yes| {
                        if yes {
                            this.start_set_parameter(parameter, new.clone());
                        } else {
                            this.0.policy.revert_parameter(parameter);
                        }
                    },
                );
            }
        }
    }

    fn start_append(&self, new: NewRule) {
        self.start("Add rule".to_owned(), OpKind::AppendRule, async move {
            let client = Client::connect_system().await?;
            let id = client
                .append_rule_after(&new.rule, new.after.as_ref(), new.persistence)
                .await?;
            Ok(OperationOutcome::RuleAppended(id))
        });
    }

    /// `exact`: the user picked this candidate from an ambiguity dialog, so
    /// it is removed by position and text, not by text alone.
    fn start_remove(&self, handle: RuleHandle, exact: bool) {
        self.start("Remove rule".to_owned(), OpKind::RemoveRule, async move {
            let client = Client::connect_system().await?;
            let outcome = if exact {
                client.remove_rule_at(&handle).await?
            } else {
                client.remove_rule(&handle).await?
            };
            Ok(OperationOutcome::RuleRemoved(outcome))
        });
    }

    fn start_set_parameter(&self, parameter: Parameter, new: String) {
        let value = new.clone();
        self.start(
            format!("Set {parameter}"),
            OpKind::SetParameter { parameter, new },
            async move {
                let client = Client::connect_system().await?;
                let previous = client.set_parameter(parameter, &value).await?;
                Ok(OperationOutcome::ParameterSet { previous })
            },
        );
    }

    /// Several rules share the text: ask which one, by evaluation position.
    fn ask_which_rule(&self, candidates: Vec<RuleHandle>) {
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Which rule?")),
            Some(&gettext(
                "Several rules have exactly this text. They differ only in their position in \
                 the evaluation order. Choose the one to remove.",
            )),
        );
        dialog.add_response("cancel", &gettext("Cancel"));
        for c in &candidates {
            let id = format!("pos-{}", c.position);
            dialog.add_response(
                &id,
                &fill(
                    &gettext("Rule {position}"),
                    &[("position", &(c.position + 1).to_string())],
                ),
            );
            dialog.set_response_appearance(&id, adw::ResponseAppearance::Destructive);
        }
        dialog.set_close_response("cancel");
        let this = self.clone();
        dialog.connect_response(None, move |_, response| {
            let chosen = candidates
                .iter()
                .find(|c| response == format!("pos-{}", c.position));
            if let Some(chosen) = chosen {
                this.start_remove(chosen.clone(), true);
            }
        });
        dialog.present(Some(&self.0.window));
    }

    fn finish(&self, op: OperationId, result: Result<OperationOutcome, AppError>) {
        // Unknown ids are operations the user cancelled; already handled.
        let Some((_, kind)) = self.0.state.borrow_mut().ops.remove(&op) else {
            return;
        };
        match (kind, result) {
            (
                OpKind::Device {
                    id,
                    policy,
                    persistence,
                    name,
                },
                result,
            ) => {
                self.0.state.borrow_mut().devices.set_pending(id, None);
                self.refresh_devices();
                match result {
                    Ok(_) => {
                        // Whole sentences, not assembled from pieces:
                        // word order differs between languages.
                        let template = match (policy, persistence) {
                            (DevicePolicy::Allow, Persistence::RuntimeOnly) => {
                                gettext("Allowed “{device}” for this session")
                            }
                            (DevicePolicy::Allow, Persistence::Permanent) => {
                                gettext("Allowed “{device}” permanently")
                            }
                            (DevicePolicy::Block, Persistence::RuntimeOnly) => {
                                gettext("Blocked “{device}” for this session")
                            }
                            (DevicePolicy::Block, Persistence::Permanent) => {
                                gettext("Blocked “{device}” permanently")
                            }
                            (DevicePolicy::Reject, Persistence::RuntimeOnly) => {
                                gettext("Rejected “{device}” for this session")
                            }
                            (DevicePolicy::Reject, Persistence::Permanent) => {
                                gettext("Rejected “{device}” permanently")
                            }
                        };
                        self.toast(&fill(&template, &[("device", &name)]));
                        if persistence == Persistence::Permanent {
                            self.mark_rules_stale();
                        }
                    }
                    Err(err) => self.report(&err, Write::Devices),
                }
            }
            (OpKind::RemoveRule, result) => {
                self.mark_rules_stale();
                match result {
                    Ok(OperationOutcome::RuleRemoved(RemoveOutcome::Removed)) => {
                        self.toast(&gettext("Rule removed"));
                    }
                    Ok(OperationOutcome::RuleRemoved(RemoveOutcome::AlreadyGone)) => {
                        self.toast(&gettext(
                            "That rule no longer exists — it was removed elsewhere. Nothing was \
                             changed.",
                        ));
                    }
                    Ok(OperationOutcome::RuleRemoved(RemoveOutcome::Ambiguous { candidates })) => {
                        self.ask_which_rule(candidates);
                    }
                    Ok(_) => {}
                    Err(err) => self.report(&err, Write::Rules),
                }
            }
            (OpKind::AppendRule, result) => {
                self.mark_rules_stale();
                match result {
                    Ok(_) => self.toast(&gettext("Rule added")),
                    Err(err) => self.report(&err, Write::Rules),
                }
            }
            (OpKind::SetParameter { parameter, new }, result) => match result {
                Ok(_) => {
                    self.0.policy.set_parameter(parameter, &new);
                    self.toast(&fill(
                        &gettext("{parameter} is now “{value}”"),
                        &[("parameter", parameter.name()), ("value", &new)],
                    ));
                }
                Err(err) => {
                    self.0.policy.revert_parameter(parameter);
                    self.report(&err, Write::Parameters);
                }
            },
        }
    }

    fn mark_rules_stale(&self) {
        self.0.state.borrow_mut().rules_stale = true;
        self.reload_policy_if_needed();
    }

    /// Reports a failed operation in its own context (§4.2, probe 6).
    fn report(&self, err: &AppError, write: Write) {
        match err {
            // Only a structural refusal — the bus policy or USBGuard's own
            // access control — says the write cannot succeed, and disables
            // the controls. Polkit also answers "Not authorized." when the
            // user dismisses the password prompt, so a Polkit refusal is
            // reported and nothing is disabled: the next attempt may well
            // succeed.
            AppError::Denied(
                state @ (AccessState::DeniedByBusPolicy | AccessState::DeniedByIpcAcl),
            ) => {
                let summary = i18n::access_summary(state);
                self.deny_write(write, &summary);
                self.toast(&fill(
                    &gettext("Not permitted: {reason}"),
                    &[("reason", &summary)],
                ));
            }
            AppError::Denied(AccessState::NoPolkitAgent) => self.toast(&gettext(
                "Not authorized, and no Polkit agent seems to be running to ask for a password. \
                 See the diagnostics.",
            )),
            AppError::Denied(_) => self.toast(&gettext(
                "Not authorized. If the password prompt was cancelled, try again; otherwise the \
                 diagnostics explain how to grant access.",
            )),
            AppError::Cancelled => self.toast(&gettext(
                "Cancelled. USBGuard may already have acted; what is shown is what it reports.",
            )),
            AppError::Stale => {
                self.toast(&gettext(
                    "The ruleset changed in the meantime, so nothing was done. It has been \
                     re-read; please choose again.",
                ));
                self.mark_rules_stale();
            }
            other => self.toast(&i18n::app_error(other)),
        }
    }

    // -----------------------------------------------------------------------
    // Events from the Tokio side
    // -----------------------------------------------------------------------

    /// The single entry point from the Tokio side.
    pub(super) fn apply(&self, event: UiEvent) {
        match event {
            UiEvent::DeviceSnapshot(devices) => {
                // A snapshot while connected is a burst resynchronization
                // (§5.4): its new devices are insertions too. The first
                // snapshot after (re)connecting announces nothing.
                let before = self.0.state.borrow().connected.then(|| self.known_ids());
                {
                    let mut state = self.0.state.borrow_mut();
                    state.connected = true;
                    state.devices.replace(devices);
                }
                self.refresh_devices();
                if let Some(before) = before {
                    self.announce_new(&before);
                }
                self.0.devices.set_stale(false);
                if !self.access_problem() {
                    self.0.banner.set_revealed(false);
                    self.set_indicator(self.connected_indicator());
                }
                self.reload_policy_if_needed();
            }
            UiEvent::DeviceBatch(batch) => {
                let before = self.known_ids();
                self.0.state.borrow_mut().devices.apply(batch);
                self.refresh_devices();
                self.announce_new(&before);
            }
            UiEvent::RuleSnapshot(rules) => self.0.policy.set_rules(rules),
            UiEvent::RuleSnapshotFailed(err) => {
                self.0.state.borrow_mut().rules_stale = true;
                self.0.policy.set_error(&i18n::app_error(&err));
            }
            UiEvent::InvalidateRuleCache => self.mark_rules_stale(),
            UiEvent::ParameterChanged { name, value } => self.parameter_changed(&name, &value),
            UiEvent::DaemonException { reason, .. } => {
                self.toast(&fill(
                    &gettext("USBGuard reported an error: {reason}"),
                    &[("reason", &reason)],
                ));
            }
            UiEvent::OperationFinished { op, result } => self.finish(op, result),
            UiEvent::AccessStateChanged(access) => self.apply_access(access),
            UiEvent::ConnectionLost(_) => {
                self.0.state.borrow_mut().connected = false;
                self.0.devices.set_stale(true);
                self.set_indicator(Indicator::Disconnected);
                if !self.access_problem() {
                    self.show_banner(
                        &gettext(
                            "Connection to USBGuard lost. Retrying… The list below may be out of \
                             date.",
                        ),
                        Some(&gettext("Details")),
                    );
                }
            }
            UiEvent::Reconnected => {
                // Nothing observed before the disconnection is trusted —
                // including which writes were refused: the daemon's
                // configuration may have changed while it was away.
                {
                    let mut state = self.0.state.borrow_mut();
                    state.devices.clear();
                    state.rules_stale = true;
                    state.denials = Denials::default();
                }
                self.0.devices.reset();
                self.apply_denials();
            }
        }
    }

    fn parameter_changed(&self, name: &str, value: &str) {
        let Some(parameter) = Parameter::from_name(name) else {
            return;
        };
        let previous = self.0.policy.set_parameter(parameter, value);
        // Our own change is announced when its operation finishes.
        let ours = self.0.state.borrow().ops.values().any(|(_, kind)| {
            matches!(kind, OpKind::SetParameter { parameter: p, new } if *p == parameter && new == value)
        });
        if !ours && previous.is_some_and(|p| p != value) {
            self.toast(&fill(
                &gettext("{parameter} was changed to “{value}”"),
                &[("parameter", name), ("value", value)],
            ));
        }
    }

    /// True when the last probe found a problem that the banner is showing.
    fn access_problem(&self) -> bool {
        self.0
            .state
            .borrow()
            .access
            .as_ref()
            .is_some_and(|a| !matches!(a, AccessState::Ok { .. }))
    }

    fn apply_access(&self, access: AccessState) {
        match &access {
            AccessState::Ok { .. } => {
                if self.0.state.borrow().connected {
                    self.set_indicator(self.connected_indicator());
                    self.0.banner.set_revealed(false);
                }
            }
            state => {
                self.set_indicator(if state.is_denied() {
                    Indicator::Denied
                } else {
                    Indicator::Disconnected
                });
                self.show_banner(
                    &fill(
                        &gettext("USBGuard: {problem}"),
                        &[("problem", &i18n::access_summary(state))],
                    ),
                    Some(&gettext("How to fix")),
                );
            }
        }
        self.0.state.borrow_mut().access = Some(access);
    }
}
