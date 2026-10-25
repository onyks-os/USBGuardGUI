use super::*;
use std::time::{Duration, Instant};

pub(in crate::ui) fn exercise(backend: &crate::ui::tests::Backend) {
    use crate::model::{AppError, DeviceId};
    use crate::rules::{parse_device, parse_rule};

    let rule = parse_rule("allow id 1234:5678").unwrap();
    let preview = super::DevicePreview::new();
    assert!(!preview.check.is_sensitive());
    preview.set_rule(Some(rule.clone()));
    assert!(preview.check.is_sensitive());
    assert!(preview.status.text().contains("Preview again"));

    // The revision token rejects a reply already queued when the user edits.
    let stale_revision = preview.revision.get();
    preview.set_rule(Some(parse_rule("block id 1234:5678").unwrap()));
    preview.finish_if_current(
        stale_revision,
        Ok(Ok(vec![parse_device(
            DeviceId::new(9),
            crate::testsupport::device_rule("allow", 1),
        )])),
    );
    assert!(preview.status.text().contains("Preview again"));
    assert!(preview.devices.first_child().is_none());

    // Device text is rendered literally, including hostile markup characters.
    preview.finish(Ok(vec![
        parse_device(
            DeviceId::new(9),
            "allow id 1234:5678 name \"<b>USB & input</b>\" via-port \"1-2\"".into(),
        ),
        parse_device(
            DeviceId::new(2),
            "block id 1234:5678 name \"Keyboard\" via-port \"1-1\"".into(),
        ),
    ]));
    assert!(preview.status.text().contains('2'));
    let rows = crate::ui::tests::widgets::<gtk::Label>(&preview.devices);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].text().contains("Keyboard"), "rows sort by topology");
    assert!(rows[1].text().contains("<b>USB & input</b>"));
    assert!(!rows[1].uses_markup());

    preview.finish(Ok(vec![]));
    assert!(preview.status.text().contains("No connected devices"));
    preview.finish(Err(AppError::Unreachable("offline".into())));
    assert!(preview.status.has_css_class("error"));
    assert!(!preview.status.text().is_empty());
    let (sender, receiver) =
        async_channel::bounded::<Result<Vec<crate::model::Device>, AppError>>(1);
    drop(sender);
    let receive_error = crate::runtime::runtime()
        .block_on(receiver.recv())
        .unwrap_err();
    preview.finish_if_current(preview.revision.get(), Err(receive_error));
    assert!(preview.status.has_css_class("error"));

    let pending_revision = preview.revision.get();
    preview.close();
    assert!(!preview.check.is_sensitive());
    let closed_status = preview.status.text();
    preview.finish_if_current(pending_revision, Ok(Ok(vec![])));
    assert_eq!(
        preview.status.text(),
        closed_status,
        "closed dialogs ignore late replies"
    );
    preview.activate();
    assert!(!preview.check.is_sensitive());
    drop(preview);

    // A cancelled in-flight request is detached from the UI and its late
    // reply cannot replace the cancellation status.
    let gate = std::sync::Arc::new(crate::testsupport::WriteGate::default());
    backend.state.lock().unwrap().device_list_gate = Some(gate.clone());
    let cancel_preview = super::DevicePreview::new();
    cancel_preview.set_rule(Some(parse_rule("allow id 1234:5678").unwrap()));
    cancel_preview.activate();
    crate::runtime::runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(3), gate.entered.notified())
            .await
            .expect("mock listDevices request did not start");
    });
    cancel_preview.activate();
    assert_eq!(cancel_preview.status.text(), "Preview cancelled.");
    backend.state.lock().unwrap().device_list_gate = None;
    gate.resume.notify_one();
    crate::ui::tests::pump_for(Duration::from_millis(100));
    assert_eq!(cancel_preview.status.text(), "Preview cancelled.");

    // Exercise the on-demand private-bus call end to end, without a password
    // prompt or policy mutation.
    let bus_preview = super::DevicePreview::new();
    bus_preview.set_rule(Some(rule));
    bus_preview.activate();
    let deadline = Instant::now() + Duration::from_secs(3);
    while bus_preview.request.borrow().is_some() && Instant::now() < deadline {
        crate::ui::tests::pump();
    }
    assert!(
        bus_preview.request.borrow().is_none(),
        "preview request timed out"
    );
    assert!(bus_preview.status.text().contains("Matching devices"));
    assert!(bus_preview.current.borrow().is_some());
    let state = backend.state.lock().unwrap();
    assert!(
        state
            .calls
            .contains(&"listDevices match id 1234:5678".into())
    );
    assert!(
        state
            .interactive
            .iter()
            .all(|(method, interactive)| { *method != "listDevices" || !interactive })
    );
}
