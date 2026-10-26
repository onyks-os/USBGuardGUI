use super::super::tests::{close_dialogs, pump, pump_for, widgets};
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
    let history = switches
        .iter()
        .find(|s| s.title() == "Keep event history between runs")
        .unwrap();
    history.set_active(true);
    assert!(config.persist_history());
    history.set_active(false);
    assert!(!config.persist_history());
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

#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise_native_close(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(900)
        .default_height(700)
        .build();
    window.present();
    let config = Config::load();
    // With animations disabled, the native close button can release the last
    // dialog reference synchronously, while GTK is still emitting clicked.
    // Drop both test references before dispatching that activation timeout.
    let settings = gtk::Settings::default().unwrap();
    let animated = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);
    window.set_default_size(900, 700);
    pump_for(std::time::Duration::from_millis(100));
    for about in [false, true] {
        if about {
            present_about(&window);
        } else {
            present_preferences(&window, &config);
        }
        pump_for(std::time::Duration::from_millis(200));
        let dialog = window.visible_dialog().unwrap();
        let weak = dialog.downgrade();
        let close = widgets::<gtk::Button>(&dialog)
            .into_iter()
            .find(|button| button.has_css_class("close"))
            .unwrap();
        assert!(close.is_mapped());
        assert!(close.activate());
        drop(close);
        drop(dialog);
        pump_for(std::time::Duration::from_millis(350));
        assert!(window.visible_dialog().is_none());
        assert!(weak.upgrade().is_none(), "closed dialog must be released");
    }
    settings.set_gtk_enable_animations(animated);
    window.close();
    pump();
}
