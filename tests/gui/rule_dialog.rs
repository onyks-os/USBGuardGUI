use super::super::tests::{button, close_dialogs, pump, widgets};
use super::*;
use crate::dbus::client::to_handles;
use std::cell::Cell;

#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder().application(app).build();
    window.present();
    pump();
    let submitted = Rc::new(Cell::new(false));
    present(&window, Vec::new(), Persistence::RuntimeOnly, {
        let submitted = submitted.clone();
        move |new| {
            assert_eq!(new.rule.to_string(), "allow");
            assert!(new.after.is_none());
            assert_eq!(new.persistence, Persistence::RuntimeOnly);
            submitted.set(true);
        }
    });
    pump();
    let dialog = window.visible_dialog().unwrap();
    let device_preview = button(&dialog, "Preview matching devices");
    assert!(device_preview.is_sensitive());
    let mode = widgets::<gtk::Stack>(&dialog)
        .into_iter()
        .find(|s| s.child_by_name("text").is_some())
        .unwrap();
    mode.set_visible_child_name("text");
    pump();
    assert!(
        device_preview.is_sensitive(),
        "preview is shared by both editor modes"
    );
    assert_eq!(
        widgets::<gtk::Entry>(&dialog)
            .into_iter()
            .find(|e| e.has_css_class("monospace"))
            .unwrap()
            .text(),
        "allow"
    );
    button(&dialog, "Add").emit_clicked();
    pump();
    assert!(submitted.get());
    close_dialogs(&window);
    // Exercise live form bindings: adding, editing and removing attributes.
    let results = Rc::new(RefCell::new(Vec::new()));
    let rules = to_handles(vec![(9, format!("allow #{}", "long ".repeat(15)))]);
    present(&window, rules.clone(), Persistence::Permanent, {
        let results = results.clone();
        move |new| results.borrow_mut().push(new)
    });
    pump();
    let dialog = window.visible_dialog().unwrap();
    let add_attribute = widgets::<gtk::Button>(&dialog)
        .into_iter()
        .find(|b| b.icon_name().as_deref() == Some("list-add-symbolic"))
        .unwrap();
    add_attribute.emit_clicked();
    pump();
    let name = widgets::<gtk::DropDown>(&dialog)
        .into_iter()
        .find(|d| d.model().unwrap().n_items() == 9)
        .unwrap();
    let entry = widgets::<gtk::Entry>(&dialog)
        .into_iter()
        .find(|e| !e.has_css_class("monospace"))
        .unwrap();
    let add = button(&dialog, "Add");
    assert!(!add.is_sensitive(), "empty id must fail local validation");
    entry.set_text("invalid");
    assert!(!add.is_sensitive());
    entry.set_text("1234:5678");
    assert!(add.is_sensitive());
    name.set_selected(
        u32::try_from(
            AttributeName::ALL
                .iter()
                .position(|a| *a == AttributeName::Name)
                .unwrap(),
        )
        .unwrap(),
    );
    entry.set_text("hostile \" name <b>");
    assert!(add.is_sensitive());
    let operator = widgets::<gtk::DropDown>(&dialog)
        .into_iter()
        .find(|d| d.model().unwrap().n_items() == 7)
        .unwrap();
    operator.set_selected(1);
    assert!(!add.is_sensitive());
    operator.set_selected(0);
    assert!(add.is_sensitive());
    let remove = widgets::<gtk::Button>(&dialog)
        .into_iter()
        .find(|b| b.icon_name().as_deref() == Some("list-remove-symbolic"))
        .unwrap();
    remove.emit_clicked();
    assert!(add.is_sensitive());
    let mode = widgets::<gtk::Stack>(&dialog)
        .into_iter()
        .find(|s| s.child_by_name("text").is_some())
        .unwrap();
    mode.set_visible_child_name("text");
    let text = widgets::<gtk::Entry>(&dialog)
        .into_iter()
        .find(|e| e.has_css_class("monospace"))
        .unwrap();
    text.set_text("allow name unquoted");
    assert!(!add.is_sensitive());
    text.set_text("reject id 1234:5678");
    assert!(add.is_sensitive());
    let position = widgets::<adw::ComboRow>(&dialog)
        .into_iter()
        .find(|r| r.title() == "Position")
        .unwrap();
    position.set_selected(1);
    add.emit_clicked();
    pump();
    assert_eq!(results.borrow().len(), 1);
    let result = &results.borrow()[0];
    assert_eq!(result.rule.to_string(), "reject id 1234:5678");
    assert_eq!(result.after, Some(rules[0].clone()));
    assert_eq!(result.persistence, Persistence::Permanent);
    close_dialogs(&window);
    present(&window, Vec::new(), Persistence::RuntimeOnly, |_| {
        panic!("cancel must not submit")
    });
    pump();
    button(&window.visible_dialog().unwrap(), "Cancel").emit_clicked();
    pump();
    close_dialogs(&window);
    // Programmatic malformed structures are caught by the round-trip guard.
    let form = Rc::new(build(Vec::new(), Persistence::RuntimeOnly));
    let refresh: Rc<dyn Fn()> = Rc::new({
        let form = form.clone();
        move || recompute(&form)
    });
    add_field_row(&form, &refresh);
    form.fields.borrow()[0]
        .entry
        .set_text(&"1234:5678 ".repeat(crate::rules::MAX_SET_VALUES + 1));
    recompute(&form);
    assert!(!form.add.is_sensitive());
    assert!(form.message.text().contains("Internal error"));
    let error = ParseError::new(crate::model::ParseErrorKind::InvalidValue, 9);
    assert!(caret_message("allow é x", error).contains("character 9"));
    window.close();
    pump();
}
