use super::super::tests::{Backend, button, close_dialogs, pump, widgets};
use super::*;
#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise(app: &adw::Application, backend: &Backend) {
    let window = adw::ApplicationWindow::builder().application(app).build();
    window.present();
    pump();
    for denied in [true, false] {
        if denied {
            backend.state.lock().unwrap().denied.insert("getParameter");
        } else {
            backend.state.lock().unwrap().denied.clear();
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
        let copy = button(&dialog, "Copy commands");
        assert_eq!(copy.is_visible(), denied);
        if denied {
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
