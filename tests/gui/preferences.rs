use super::super::tests::{close_dialogs, pump, widgets};
use super::*;
#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder().application(app).build();
    window.present();
    pump();
    let config = Config::load();
    present_preferences(&window, &config);
    pump();
    let dialog = window.visible_dialog().unwrap();
    let persistence = widgets::<adw::ComboRow>(&dialog)[0].clone();
    persistence.set_selected(1);
    assert_eq!(
        config.default_persistence(),
        crate::model::Persistence::Permanent
    );
    persistence.set_selected(0);
    let switches = widgets::<adw::SwitchRow>(&dialog);
    let login = switches
        .iter()
        .find(|s| s.title() == "Start at login")
        .unwrap();
    login.set_active(true);
    assert!(autostart::enabled());
    login.set_active(false);
    assert!(!autostart::enabled());
    let notify = switches
        .iter()
        .find(|s| s.title() == "Announce new devices")
        .unwrap();
    notify.set_active(false);
    assert!(!config.notify_inserted());
    notify.set_active(true);
    close_dialogs(&window);
    let defaults = super::super::config::tests::without_schema();
    present_preferences(&window, &defaults);
    pump();
    let dialog = window.visible_dialog().unwrap();
    assert!(
        widgets::<adw::ComboRow>(&dialog)
            .iter()
            .all(|r| !r.is_sensitive())
    );
    assert!(
        widgets::<adw::SwitchRow>(&dialog)
            .iter()
            .filter(|s| s.title() != "Start at login")
            .all(|s| !s.is_sensitive())
    );
    close_dialogs(&window);
    present_about(&window);
    pump();
    let about = window
        .visible_dialog()
        .unwrap()
        .downcast::<adw::AboutDialog>()
        .unwrap();
    assert_eq!(about.version(), crate::VERSION);
    close_dialogs(&window);
    window.close();
    pump();
}
