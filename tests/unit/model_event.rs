use super::UiEvent;

fn assert_send<T: Send + 'static>() {}

#[test]
fn ui_event_crosses_threads() {
    assert_send::<UiEvent>();
}
