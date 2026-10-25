//! StatusNotifierItem and its fixed D-Bus menu on the application's zbus stack.
//!
//! Only immutable counts and channel commands cross the Tokio/GTK boundary.
//! Registration fails explicitly when no watcher/host is available, preserving
//! background mode without an invisible icon. No second D-Bus stack is needed.

use std::collections::HashMap;
use std::sync::Arc;

use async_channel::Sender;
use futures_util::StreamExt;
use gettextrs::gettext;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use zbus::{Connection, ObjectPath, OwnedVariant, Type, Variant, fdo};

use super::i18n::fill;
use super::{APP_ID, APP_NAME};
use crate::runtime::runtime;

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
type Properties = HashMap<String, OwnedVariant>;
type Pixmaps = Vec<(i32, i32, Vec<u8>)>;
type ToolTip = (String, Pixmaps, String, String);
type MenuEvent = (i32, String, OwnedVariant, u32);

/// What the tray asks the GTK side to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TrayCommand {
    Show,
    Quit,
}

struct State {
    commands: Sender<TrayCommand>,
    counts: RwLock<(usize, usize)>,
    show: String,
    quit: String,
}

impl State {
    fn command(&self, command: TrayCommand) {
        let _ = self.commands.try_send(command);
    }

    fn properties(&self, id: i32, requested: &[String]) -> fdo::Result<Properties> {
        let text = |value: &str| OwnedVariant::from(zbus::Str::from(value.to_owned()));
        let mut properties = Properties::new();
        match id {
            0 => {
                properties.insert("children-display".into(), text("submenu"));
            }
            1 | 3 => {
                properties.insert(
                    "label".into(),
                    text(if id == 1 { &self.show } else { &self.quit }),
                );
                properties.insert("enabled".into(), true.into());
                properties.insert("visible".into(), true.into());
                if id == 3 {
                    properties.insert("icon-name".into(), text("application-exit"));
                }
            }
            2 => {
                properties.insert("type".into(), text("separator"));
            }
            _ => return Err(fdo::Error::InvalidArgs("Unknown menu item".into())),
        }
        if !requested.is_empty() {
            properties.retain(|name, _| requested.contains(name));
        }
        Ok(properties)
    }

    fn event(&self, id: i32, event: &str) -> fdo::Result<()> {
        if !(0..=3).contains(&id) {
            return Err(fdo::Error::InvalidArgs("Unknown menu item".into()));
        }
        if event == "clicked" {
            match id {
                1 => self.command(TrayCommand::Show),
                3 => self.command(TrayCommand::Quit),
                _ => (),
            }
        }
        Ok(())
    }
}

struct Item(Arc<State>);

// The protocol requires instance methods even for immutable properties.
#[allow(clippy::unused_self)]
#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    fn activate(&self, x: i32, y: i32) {
        let _ = (x, y);
        self.0.command(TrayCommand::Show);
    }
    fn secondary_activate(&self, x: i32, y: i32) {
        let _ = (x, y);
    }
    fn context_menu(&self, x: i32, y: i32) -> fdo::Result<()> {
        let _ = (x, y);
        Err(fdo::Error::UnknownMethod(
            "Use the exported D-Bus menu".into(),
        ))
    }
    fn scroll(&self, delta: i32, orientation: &str) -> fdo::Result<()> {
        let _ = delta;
        if matches!(orientation, "vertical" | "horizontal") {
            Ok(())
        } else {
            Err(fdo::Error::InvalidArgs("Unknown scroll orientation".into()))
        }
    }
    #[zbus(property)]
    fn category(&self) -> &'static str {
        "ApplicationStatus"
    }
    #[zbus(property)]
    fn id(&self) -> &'static str {
        APP_ID
    }
    #[zbus(property)]
    fn title(&self) -> &'static str {
        APP_NAME
    }
    #[zbus(property)]
    fn status(&self) -> &'static str {
        "Active"
    }
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn icon_name(&self) -> String {
        format!("{APP_ID}-symbolic")
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> &'static str {
        ""
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        Vec::new()
    }
    #[zbus(property)]
    fn overlay_icon_name(&self) -> &'static str {
        ""
    }
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Pixmaps {
        Vec::new()
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> &'static str {
        ""
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Pixmaps {
        Vec::new()
    }
    #[zbus(property)]
    fn attention_movie_name(&self) -> &'static str {
        ""
    }
    #[zbus(property)]
    fn menu(&self) -> ObjectPath<'static> {
        ObjectPath::from_static_str_unchecked(MENU_PATH)
    }
    #[zbus(property)]
    async fn tool_tip(&self) -> ToolTip {
        let (total, blocked) = *self.0.counts.read().await;
        (
            self.icon_name(),
            Vec::new(),
            APP_NAME.into(),
            fill(
                &gettext("{total} USB devices, {blocked} not authorized"),
                &[
                    ("total", &total.to_string()),
                    ("blocked", &blocked.to_string()),
                ],
            ),
        )
    }
    #[zbus(signal)]
    async fn new_tool_tip(emitter: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

#[derive(Debug, Serialize, Deserialize, Type)]
struct Layout {
    id: i32,
    properties: Properties,
    children: Vec<OwnedVariant>,
}

struct Menu(Arc<State>);

// The macro decodes owned arrays for D-Bus arguments.
#[allow(clippy::unused_self, clippy::needless_pass_by_value)]
#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        property_names: Vec<String>,
    ) -> fdo::Result<(u32, Layout)> {
        let properties = self.0.properties(parent_id, &property_names)?;
        let mut children = Vec::new();
        if parent_id == 0 && recursion_depth != 0 {
            for id in 1..=3 {
                let child = Layout {
                    id,
                    properties: self.0.properties(id, &property_names)?,
                    children: Vec::new(),
                };
                children.push(
                    OwnedVariant::try_from(Variant::new((
                        child.id,
                        child.properties,
                        child.children,
                    )))
                    .map_err(|error| fdo::Error::Failed(error.to_string()))?,
                );
            }
        }
        Ok((
            1,
            Layout {
                id: parent_id,
                properties,
                children,
            },
        ))
    }
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        property_names: Vec<String>,
    ) -> Vec<(i32, Properties)> {
        let ids = if ids.is_empty() {
            vec![0, 1, 2, 3]
        } else {
            ids
        };
        ids.into_iter()
            .filter_map(|id| {
                self.0
                    .properties(id, &property_names)
                    .ok()
                    .map(|properties| (id, properties))
            })
            .collect()
    }
    fn get_property(&self, id: i32, name: &str) -> fdo::Result<OwnedVariant> {
        self.0
            .properties(id, &[])?
            .remove(name)
            .ok_or_else(|| fdo::Error::InvalidArgs("Unknown menu property".into()))
    }
    fn event(
        &self,
        id: i32,
        event_id: &str,
        data: OwnedVariant,
        timestamp: u32,
    ) -> fdo::Result<()> {
        let _ = (data, timestamp);
        self.0.event(id, event_id)
    }
    fn event_group(&self, events: Vec<MenuEvent>) -> Vec<i32> {
        events
            .into_iter()
            .filter_map(|(id, event, _, _)| self.0.event(id, &event).err().map(|_| id))
            .collect()
    }
    fn about_to_show(&self, id: i32) -> fdo::Result<bool> {
        self.0.properties(id, &[]).map(|_| false)
    }
    fn about_to_show_group(&self, ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (
            Vec::new(),
            ids.into_iter().filter(|id| !(0..=3).contains(id)).collect(),
        )
    }
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> &'static str {
        "ltr"
    }
    #[zbus(property)]
    fn status(&self) -> &'static str {
        "normal"
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Owns the tray connection and the state behind its exported interfaces.
#[derive(Clone)]
pub(super) struct TrayHandle {
    connection: Connection,
    state: Arc<State>,
}

/// The running tray, or why there is none.
pub(super) type TrayResult = Result<TrayHandle, String>;

async fn register(
    connection: Connection,
    commands: Sender<TrayCommand>,
    sandboxed: bool,
) -> zbus::Result<TrayHandle> {
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut changes = bus
        .receive_name_owner_changed_with_args(&[(0, WATCHER)])
        .await?;
    if !bus
        .name_has_owner(zbus::names::BusName::try_from(WATCHER)?)
        .await?
    {
        return Err(fdo::Error::Failed("No status notifier watcher".into()).into());
    }
    let watcher = zbus::Proxy::new(&connection, WATCHER, "/StatusNotifierWatcher", WATCHER).await?;
    if !watcher
        .get_property::<bool>("IsStatusNotifierHostRegistered")
        .await?
    {
        return Err(fdo::Error::Failed("No status notifier host".into()).into());
    }
    let state = Arc::new(State {
        commands,
        counts: RwLock::new((0, 0)),
        show: fill(&gettext("Show {app}"), &[("app", APP_NAME)]),
        quit: gettext("Quit"),
    });
    connection
        .object_server()
        .at(ITEM_PATH, Item(state.clone()))
        .await?;
    connection
        .object_server()
        .at(MENU_PATH, Menu(state.clone()))
        .await?;
    let service = if sandboxed {
        connection
            .unique_name()
            .ok_or_else(|| fdo::Error::Failed("Tray needs a bus connection".into()))?
            .to_string()
    } else {
        let name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
        connection.request_name(name.as_str()).await?;
        name
    };
    watcher
        .call::<_, _, ()>("RegisterStatusNotifierItem", &(service.as_str(),))
        .await?;
    // A restarted desktop watcher loses its item list. Register the same
    // exported objects again, without polling or touching GTK from this task.
    let connection_for_watch = connection.clone();
    runtime().spawn(async move {
        while let Some(signal) = changes.next().await {
            if signal.args().is_ok_and(|args| args.new_owner().is_some())
                && let Ok(watcher) = zbus::Proxy::new(
                    &connection_for_watch,
                    WATCHER,
                    "/StatusNotifierWatcher",
                    WATCHER,
                )
                .await
            {
                let _ = watcher
                    .call::<_, _, ()>("RegisterStatusNotifierItem", &(service.as_str(),))
                    .await;
            }
        }
    });
    drop(watcher);
    drop(bus);
    Ok(TrayHandle { connection, state })
}

/// Registers the icon. The answer arrives on `reply`.
pub(super) fn start(commands: Sender<TrayCommand>, reply: Sender<TrayResult>) {
    runtime().spawn(async move {
        let result = match Connection::session().await {
            Ok(connection) => register(connection, commands, super::autostart::sandboxed()).await,
            Err(error) => Err(error),
        };
        let _ = reply.send(result.map_err(|error| error.to_string())).await;
    });
}

async fn update_counts(handle: &TrayHandle, total: usize, blocked: usize) -> zbus::Result<()> {
    *handle.state.counts.write().await = (total, blocked);
    let emitter = zbus::object_server::SignalEmitter::new(&handle.connection, ITEM_PATH)?;
    Item::new_tool_tip(&emitter).await?;
    let item = handle
        .connection
        .object_server()
        .interface::<_, Item>(ITEM_PATH)
        .await?;
    item.get().await.tool_tip_changed(&emitter).await
}

/// Updates the tooltip counts and notifies the watcher.
pub(super) fn update(handle: &TrayHandle, total: usize, blocked: usize) {
    let handle = handle.clone();
    runtime().spawn(async move {
        let _ = update_counts(&handle, total, blocked).await;
    });
}

/// Removes the icon and closes its dedicated connection.
pub(super) fn stop(handle: &TrayHandle) {
    let handle = handle.clone();
    runtime().spawn(async move {
        let _ = handle.connection.close().await;
    });
}

#[cfg(test)]
mod tests;
