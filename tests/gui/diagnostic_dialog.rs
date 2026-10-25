use super::super::tests::{Backend, button, close_dialogs, pump, widgets};
use super::*;
#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise(app: &adw::Application, backend: &Backend) {
    let window = adw::ApplicationWindow::builder().application(app).build();
    window.present();
    pump();
    // A headless CI runner has no Polkit agent, while a desktop often does.
    // IPC denial supplies a command-bearing remedy independently of that heuristic.
    for failure in [Some("polkit"), Some("ipc"), None] {
        {
            let mut state = backend.state.lock().unwrap();
            state.denied.clear();
            state.failures.clear();
            match failure {
                Some("polkit") => {
                    state.denied.insert("getParameter");
                }
                Some("ipc") => {
                    state
                        .failures
                        .insert("getParameter", "IPC access denied".into());
                }
                _ => {}
            }
        }
        present(&window);
        pump();
        let dialog = window.visible_dialog().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !widgets::<gtk::Label>(&dialog)
            .iter()
            .any(|l| l.text().contains("Result:"))
        {
            assert!(std::time::Instant::now() < deadline);
            pump();
        }
        let report = widgets::<gtk::Label>(&dialog)
            .into_iter()
            .find(|label| label.text().contains("Result:"))
            .unwrap()
            .text();
        match failure {
            Some("polkit") => assert!(
                report.contains("Result: denied by Polkit")
                    || report.contains("Result: no Polkit authentication agent")
            ),
            Some("ipc") => {
                assert!(report.contains("Result: denied by the USBGuard IPC access control"));
            }
            _ => assert!(report.contains("Result: connected")),
        }
        let has_commands = report
            .lines()
            .any(|line| line.trim_start().starts_with("$ "));
        let copy = button(&dialog, "Copy commands");
        assert_eq!(copy.is_visible(), has_commands);
        if has_commands {
            copy.emit_clicked();
            let text = glib::MainContext::default()
                .block_on(copy.clipboard().read_text_future())
                .unwrap()
                .unwrap();
            assert!(text.contains("sudo "));
        }
        close_dialogs(&window);
    }
    // The installed/absent distinction is read from the private real bus.
    crate::runtime::runtime()
        .block_on(backend.connection.release_name("org.usbguard1"))
        .unwrap();
    present(&window);
    pump();
    close_dialogs(&window);
    crate::runtime::runtime()
        .block_on(backend.connection.request_name("org.usbguard1"))
        .unwrap();
    window.close();
    pump();
}
