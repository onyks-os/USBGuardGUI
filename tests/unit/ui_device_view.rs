use std::cell::OnceCell;
use std::time::{Duration, Instant};

use super::*;
use crate::rules::parse_device;

fn pump_for(duration: Duration) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn find_widget<T: IsA<gtk::Widget> + glib::object::ObjectType>(
    root: &gtk::Widget,
    predicate: &impl Fn(&T) -> bool,
) -> Option<T> {
    if let Ok(widget) = root.clone().downcast::<T>()
        && predicate(&widget)
    {
        return Some(widget);
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(found) = find_widget(&widget, predicate) {
            return Some(found);
        }
        child = widget.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run on a private session bus with GSETTINGS_BACKEND=memory"]
#[allow(clippy::unwrap_used)]
fn gui_device_actions_survive_denied_authentication_and_refresh() {
    gtk::init().unwrap();
    super::super::i18n::init();
    let block_label = gettext("Block");
    let session_label = gettext("This session only");
    if std::env::var("LANGUAGE").as_deref() == Ok("fr") {
        assert_eq!(block_label, "Bloquer");
        assert_eq!(session_label, "Pour cette session uniquement");
    }
    let slot: Rc<OnceCell<std::rc::Weak<DeviceView>>> = Rc::default();
    let device = parse_device(
        DeviceId::new(3),
        "allow id 1234:5678 name \"Test device\"".into(),
    );
    let view = Rc::new(DeviceView::new(
        &Config::load(),
        Rc::new({
            let (slot, device) = (slot.clone(), device.clone());
            move |action| {
                assert_eq!(
                    action,
                    DeviceAction::Apply(device.id, DevicePolicy::Block, Persistence::RuntimeOnly)
                );
                let mut pending = device.clone();
                pending.pending = Some(PendingKind::Started);
                slot.get()
                    .unwrap()
                    .upgrade()
                    .unwrap()
                    .set_devices(vec![pending]);
            }
        }),
        Rc::default(),
    ));
    slot.set(Rc::downgrade(&view)).unwrap();
    let window = gtk::Window::builder()
        .default_width(1000)
        .default_height(400)
        .child(&view.root)
        .build();
    view.set_devices(vec![device.clone()]);
    window.present();
    pump_for(Duration::from_millis(150));
    for _ in 0..20 {
        let block = find_widget::<gtk::MenuButton>(view.root.upcast_ref(), &|b| {
            b.label().as_deref() == Some(block_label.as_str())
        })
        .unwrap();
        let popover = block.popover().unwrap();
        block.grab_focus();
        block.popup();
        pump_for(Duration::from_millis(100));
        let session = find_widget::<gtk::Button>(popover.upcast_ref(), &|b| {
            b.label().as_deref() == Some(session_label.as_str())
        })
        .unwrap();
        session.grab_focus();
        session.emit_clicked();
        drop(session);
        drop(popover);
        drop(block);
        let authentication = gtk::Window::builder()
            .title("Test authentication")
            .transient_for(&window)
            .build();
        authentication.present();
        pump_for(Duration::from_millis(150));
        // Polkit cancellation returns a refusal and clears the pending row.
        view.set_devices(vec![device.clone()]);
        authentication.close();
        pump_for(Duration::from_millis(150));
    }
    // Reconnection also removes the old rows, including while focus is
    // held by another window. It must clear the same stale focus chain.
    let block = find_widget::<gtk::MenuButton>(view.root.upcast_ref(), &|b| {
        b.label().as_deref() == Some(block_label.as_str())
    })
    .unwrap();
    block.grab_focus();
    drop(block);
    let authentication = gtk::Window::builder().transient_for(&window).build();
    authentication.present();
    pump_for(Duration::from_millis(100));
    view.reset();
    authentication.close();
    pump_for(Duration::from_millis(100));
    view.set_devices(vec![device.clone()]);
    pump_for(Duration::from_millis(100));
    // Refreshes must not interrupt someone typing in the toolbar.
    let search = find_widget::<gtk::SearchEntry>(view.root.upcast_ref(), &|_| true).unwrap();
    search.grab_focus();
    let focused = gtk::prelude::GtkWindowExt::focus(&window).unwrap();
    view.set_devices(vec![device]);
    assert_eq!(
        gtk::prelude::GtkWindowExt::focus(&window).as_ref(),
        Some(&focused)
    );
    window.close();
    pump_for(Duration::from_millis(100));
}

#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise() {
    use super::super::tests::{button, widgets};
    let config = Config::load();
    let actions = Rc::new(RefCell::new(Vec::new()));
    let block: ModifyBlock = Rc::default();
    let view = DeviceView::new(
        &config,
        Rc::new({
            let actions = actions.clone();
            move |a| actions.borrow_mut().push(a)
        }),
        block.clone(),
    );
    let window = gtk::Window::builder()
        .default_width(1200)
        .default_height(700)
        .child(&view.root)
        .build();
    let mut devices = vec![
        parse_device(
            DeviceId::new(1),
            "allow id 1234:0001 name \"<b>Visible & USB</b>\" serial \"SN-1\" via-port \"2-3.1\" with-interface { 08:06:50 03:01:01 } with-connect-type \"hotplug\"".into(),
        ),
        parse_device(DeviceId::new(2), "block serial \"SERIAL\"".into()),
        parse_device(DeviceId::new(3), "reject name \"\"".into()),
        parse_device(DeviceId::new(4), "bad syntax".into()),
    ];
    devices[3].target = Target::Other(99);
    view.set_devices(devices.clone());
    window.present();
    pump_for(Duration::from_millis(100));
    let columns = widgets::<gtk::ColumnView>(&view.root)[0].clone();
    assert_eq!(columns.model().unwrap().n_items(), 4);
    let details = widgets::<gtk::MenuButton>(&view.root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some("Details"))
        .unwrap();
    details.popup();
    pump_for(Duration::from_millis(40));
    let detail_label = widgets::<gtk::Label>(&details.popover().unwrap())
        .into_iter()
        .find(|label| label.text().contains("Matched policy rule"))
        .unwrap();
    let detail_text = detail_label.text();
    assert!(detail_text.contains("State reported by USBGuard: Allowed"));
    assert!(detail_text.contains("Physical port path: 2-3.1"));
    assert!(detail_text.contains("08:06:50, 03:01:01"));
    assert!(detail_text.contains("<b>Visible & USB</b>"));
    assert!(!detail_label.uses_markup());
    assert!(detail_text.contains("Not provided in the listDevices response."));
    details.popdown();

    view.set_devices(vec![devices[3].clone()]);
    pump_for(Duration::from_millis(40));
    let malformed_details = widgets::<gtk::MenuButton>(&view.root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some("Details"))
        .unwrap();
    malformed_details.popup();
    pump_for(Duration::from_millis(40));
    let malformed_text = widgets::<gtk::Label>(&malformed_details.popover().unwrap())
        .into_iter()
        .find(|label| label.text().contains("Rule parsing issue"))
        .unwrap()
        .text();
    assert!(malformed_text.contains("Unknown"));
    assert!(malformed_text.contains("Rule parsing issue:"));
    assert!(malformed_text.to_lowercase().contains("expected"));
    assert!(malformed_text.contains("USB ID: —"));
    malformed_details.popdown();

    view.set_devices(vec![devices[2].clone()]);
    pump_for(Duration::from_millis(40));
    let sparse_details = widgets::<gtk::MenuButton>(&view.root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some("Details"))
        .unwrap();
    sparse_details.popup();
    pump_for(Duration::from_millis(40));
    let sparse_text = widgets::<gtk::Label>(&sparse_details.popover().unwrap())
        .into_iter()
        .find(|label| label.text().contains("Matched policy rule"))
        .unwrap()
        .text();
    assert!(sparse_text.contains("USB interfaces (class:subclass:protocol): —"));
    assert!(sparse_text.contains("Physical port path: —"));
    sparse_details.popdown();

    view.set_devices(devices.clone());
    pump_for(Duration::from_millis(40));
    let search = widgets::<gtk::SearchEntry>(&view.root)[0].clone();
    search.set_text("serial");
    search.emit_by_name::<()>("search-changed", &[]);
    assert_eq!(columns.model().unwrap().n_items(), 1);
    search.set_text("");
    search.emit_by_name::<()>("search-changed", &[]);
    let blocked = widgets::<gtk::ToggleButton>(&view.root)[0].clone();
    blocked.set_active(true);
    assert_eq!(columns.model().unwrap().n_items(), 3);
    blocked.set_active(false);
    for pending in [
        PendingKind::Started,
        PendingKind::WaitingForAuthentication,
        PendingKind::PossiblyNoAgent,
    ] {
        let mut d = devices[1].clone();
        d.pending = Some(pending);
        view.set_devices(vec![d]);
        pump_for(Duration::from_millis(40));
        button(&view.root, "Cancel").emit_clicked();
        assert_eq!(
            actions.borrow().last(),
            Some(&DeviceAction::Cancel(DeviceId::new(2)))
        );
    }
    view.set_devices(vec![devices[0].clone()]);
    pump_for(Duration::from_millis(40));
    config
        .settings()
        .unwrap()
        .set_enum("default-persistence", 1)
        .unwrap();
    let allow = widgets::<gtk::MenuButton>(&view.root)
        .into_iter()
        .find(|b| b.label().as_deref() == Some("Allow"))
        .unwrap();
    allow.popup();
    pump_for(Duration::from_millis(40));
    let permanent = button(&allow.popover().unwrap(), "Permanently");
    assert!(permanent.has_css_class("suggested-action"));
    permanent.emit_clicked();
    assert_eq!(
        actions.borrow().last(),
        Some(&DeviceAction::Apply(
            DeviceId::new(1),
            DevicePolicy::Allow,
            Persistence::Permanent
        ))
    );
    *block.borrow_mut() = Some("denied".into());
    view.set_devices(vec![devices[0].clone()]);
    view.set_stale(true);
    assert!(!view.list_box.is_sensitive());
    view.set_stale(false);
    assert!(view.list_box.is_sensitive());
    view.set_devices(Vec::new());
    assert_eq!(view.content.visible_child_name().as_deref(), Some("empty"));
    view.reset();
    assert_eq!(
        view.content.visible_child_name().as_deref(),
        Some("loading")
    );
    config.settings().unwrap().reset("default-persistence");
    window.close();
    pump_for(Duration::from_millis(30));
}
