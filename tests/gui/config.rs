use super::*;
#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise() {
    let defaults = Config(None);
    assert!(!defaults.run_in_background());
    assert!(defaults.notify_inserted());
    assert!(!defaults.start_in_background());
    assert_eq!(defaults.default_persistence(), Persistence::RuntimeOnly);
    assert_eq!(defaults.window_size(), (940, 600));
    assert!(!defaults.window_maximized());
    assert!(defaults.take_background_notice());
    defaults.save_window(800, 500, true);
    let config = Config::load();
    let s = config.settings().unwrap();
    assert!(config.take_background_notice());
    assert!(!config.take_background_notice());
    s.set_boolean("run-in-background", true).unwrap();
    assert!(config.run_in_background());
    s.set_boolean("start-in-background", true).unwrap();
    assert!(config.start_in_background());
    s.set_boolean("notify-inserted", false).unwrap();
    assert!(!config.notify_inserted());
    s.set_enum("default-persistence", 1).unwrap();
    assert_eq!(config.default_persistence(), Persistence::Permanent);
    config.save_window(800, 500, true);
    assert_eq!(config.window_size(), (800, 500));
    assert!(config.window_maximized());
    for key in s.settings_schema().unwrap().list_keys() {
        s.reset(&key);
    }
}

pub(in crate::ui) fn without_schema() -> Config {
    Config(None)
}

#[test]
fn development_schema_is_available_without_a_system_install() {
    assert!(development_schema().is_some());
}
