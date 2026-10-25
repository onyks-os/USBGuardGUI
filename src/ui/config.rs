//! Persisted settings through GSettings (docs/architecture.md §10.1).
//!
//! No setting here has any effect on the daemon: nothing is security policy.
//! When no schema can be found — a development build without
//! `glib-compile-schemas`, say — every getter returns its documented default
//! and nothing is persisted, rather than aborting as `gio::Settings::new` would.

use adw::prelude::*;
use gtk::gio;
use tracing::warn;

use super::APP_ID;
use crate::model::Persistence;

/// The application's settings, or their defaults.
#[derive(Clone)]
pub(super) struct Config(Option<gio::Settings>);

impl Config {
    pub(super) fn load() -> Self {
        let installed = gio::SettingsSchemaSource::default().and_then(|s| s.lookup(APP_ID, true));
        // Prefer the schema staged by this checkout in debug builds. An older
        // system-installed copy may otherwise win and lack newly added keys.
        let schema = development_schema().or(installed);
        if schema.is_none() {
            warn!("GSettings schema {APP_ID} not found; settings will not be saved");
        }
        Self(
            schema.map(|schema| {
                gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None)
            }),
        )
    }

    /// The underlying settings, for property bindings.
    pub(super) const fn settings(&self) -> Option<&gio::Settings> {
        self.0.as_ref()
    }

    fn bool(&self, key: &str, default: bool) -> bool {
        self.0.as_ref().map_or(default, |s| s.boolean(key))
    }

    fn int(&self, key: &str, default: i32) -> i32 {
        self.0.as_ref().map_or(default, |s| s.int(key))
    }

    pub(super) fn run_in_background(&self) -> bool {
        self.bool("run-in-background", false)
    }

    pub(super) fn notify_inserted(&self) -> bool {
        self.bool("notify-inserted", true)
    }

    pub(super) fn persist_history(&self) -> bool {
        self.bool("persist-history", false)
    }

    pub(super) fn history_data(&self) -> String {
        self.0
            .as_ref()
            .map_or_else(String::new, |s| s.string("history-data").to_string())
    }

    pub(super) fn save_history(&self, value: &str) {
        if self.persist_history()
            && let Some(settings) = &self.0
        {
            let _ = settings.set_string("history-data", value);
        }
    }

    pub(super) fn set_persist_history(&self, enabled: bool) {
        if let Some(settings) = &self.0 {
            let _ = settings.set_boolean("persist-history", enabled);
            if !enabled {
                // Opting out also removes the previously persisted copy.
                let _ = settings.set_string("history-data", "");
            }
        }
    }

    /// Whether the one-time "still running in the background" notice was
    /// given; marks it given.
    pub(super) fn take_background_notice(&self) -> bool {
        let shown = self.bool("tray-notice-shown", false);
        if let Some(s) = &self.0 {
            let _ = s.set_boolean("tray-notice-shown", true);
        }
        !shown
    }

    pub(super) fn start_in_background(&self) -> bool {
        self.bool("start-in-background", false)
    }

    pub(super) fn default_persistence(&self) -> Persistence {
        match self.0.as_ref().map(|s| s.enum_("default-persistence")) {
            Some(1) => Persistence::Permanent,
            _ => Persistence::RuntimeOnly,
        }
    }

    pub(super) fn window_size(&self) -> (i32, i32) {
        (
            self.int("window-width", 940),
            self.int("window-height", 600),
        )
    }

    pub(super) fn window_maximized(&self) -> bool {
        self.bool("window-maximized", false)
    }

    /// Saves the window geometry; called when the window closes.
    pub(super) fn save_window(&self, width: i32, height: i32, maximized: bool) {
        if let Some(s) = &self.0 {
            let _ = s.set_int("window-width", width);
            let _ = s.set_int("window-height", height);
            let _ = s.set_boolean("window-maximized", maximized);
        }
    }
}

/// The schema compiled by `build.rs`, for `cargo run` from the source tree.
fn development_schema() -> Option<gio::SettingsSchema> {
    let dir = option_env!("USBGUARD_GUI_DEV_SCHEMA_DIR")?;
    let source = gio::SettingsSchemaSource::from_directory(
        dir,
        gio::SettingsSchemaSource::default().as_ref(),
        false,
    )
    .ok()?;
    source.lookup(APP_ID, false)
}

#[cfg(test)]
#[path = "../../tests/gui/config.rs"]
pub(super) mod tests;
