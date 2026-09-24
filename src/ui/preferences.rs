//! Preferences and About (docs/architecture.md §10.1).
//!
//! Only settings with a visible effect are offered; `start-in-background` is
//! reached through "Start at login", which starts the program hidden.

use adw::prelude::*;

use super::config::Config;
use super::{APP_ID, autostart};

pub(super) fn present_preferences(parent: &impl IsA<gtk::Widget>, config: &Config) {
    let persistence = adw::ComboRow::builder()
        .title("Preselected choice for device actions")
        .subtitle(
            "“This session only” is the safer default: its effect disappears when USBGuard \
             restarts.",
        )
        .model(&gtk::StringList::new(&["This session only", "Permanently"]))
        .build();
    let group = adw::PreferencesGroup::builder()
        .title("Device actions")
        .build();
    group.add(&persistence);

    if let Some(settings) = config.settings() {
        persistence.set_selected(u32::try_from(settings.enum_("default-persistence")).unwrap_or(0));
        let settings = settings.clone();
        persistence.connect_selected_notify(move |row| {
            let _ = settings.set_enum(
                "default-persistence",
                i32::try_from(row.selected()).unwrap_or(0),
            );
        });
    } else {
        persistence.set_sensitive(false);
        persistence.set_tooltip_text(Some(
            "Settings cannot be saved: the GSettings schema is not installed",
        ));
    }

    let notify = adw::SwitchRow::builder()
        .title("Announce new devices")
        .subtitle("Notify when a device that is not authorized is plugged in")
        .build();
    let background = adw::SwitchRow::builder()
        .title("Keep running in the background")
        .subtitle("After the window is closed, keep announcing new devices")
        .build();
    let login = adw::SwitchRow::builder()
        .title("Start at login")
        .subtitle("Start hidden when you log in, so the first device is announced too")
        .active(autostart::enabled())
        .visible(!autostart::sandboxed())
        .build();
    login.connect_active_notify(|row| {
        if let Err(e) = autostart::set(row.is_active()) {
            tracing::warn!("could not change the autostart entry: {e}");
        }
    });
    if let Some(settings) = config.settings() {
        settings.bind("notify-inserted", &notify, "active").build();
        settings
            .bind("run-in-background", &background, "active")
            .build();
    } else {
        notify.set_sensitive(false);
        background.set_sensitive(false);
    }
    let session = adw::PreferencesGroup::builder().title("Background").build();
    session.add(&notify);
    session.add(&background);
    session.add(&login);

    let page = adw::PreferencesPage::new();
    page.add(&group);
    page.add(&session);
    let dialog = adw::PreferencesDialog::new();
    dialog.add(&page);
    dialog.present(Some(parent));
}

pub(super) fn present_about(parent: &impl IsA<gtk::Widget>) {
    let about = adw::AboutDialog::builder()
        .application_name("USBGuard")
        .application_icon(APP_ID)
        .developer_name("onyks-os")
        .version(crate::VERSION)
        .website("https://github.com/onyks-os/USBGuardGUI")
        .issue_url("https://github.com/onyks-os/USBGuardGUI/issues")
        .license_type(gtk::License::MitX11)
        .comments(
            "An unprivileged interface to the USBGuard daemon. Every change is made by the \
             daemon, after the system's own authorization checks.",
        )
        .build();
    about.present(Some(parent));
}
