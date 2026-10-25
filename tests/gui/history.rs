use super::*;

#[allow(clippy::unwrap_used)]
#[test]
fn bounded_history_round_trips_and_exports_no_device_identifiers() {
    let mut history = History::default();
    for _ in 0..250 {
        history.record_inserted();
        history.record_gap();
    }
    assert_eq!(history.events.len(), CAPACITY);
    let decoded = History::from_setting(&history.encode());
    assert_eq!(decoded.events, history.events);
    let json = decoded.export_json();
    assert!(json.contains("\"identifiers_included\": false"));
    assert!(!json.contains("serial"));
    assert!(!json.contains("hash"));
    assert!(!json.contains("name"));
    assert_eq!(json.matches("\"timestamp_unix\"").count(), CAPACITY);
}

#[allow(clippy::unwrap_used)]
#[test]
fn malformed_persisted_history_is_ignored_and_repeated_gaps_are_coalesced() {
    let value = format!("bad\nnot-a-time|i\n{}|i\n1|unknown\n2|i\n", "x".repeat(33));
    let mut history = History::from_setting(&value);
    assert_eq!(history.events.len(), 1);
    history.record_gap();
    history.record_gap();
    assert_eq!(history.events.len(), 2);
    assert_eq!(
        history.events.last().unwrap().kind,
        EventKind::ConnectionGap
    );
    history.record_inserted();
    history.record_burst_gap();
    history.record_burst_gap();
    assert_eq!(history.events.len(), 4);
    assert_eq!(history.events.last().unwrap().kind, EventKind::BurstGap);
}

#[allow(clippy::unwrap_used)]
#[test]
fn every_event_kind_round_trips_and_export_file_failures_are_safe() {
    let kinds = [
        EventKind::Inserted,
        EventKind::Removed,
        EventKind::Authorized,
        EventKind::Blocked,
        EventKind::Rejected,
        EventKind::ActionAllowed,
        EventKind::ActionBlocked,
        EventKind::ActionRejected,
        EventKind::ActionFailed,
        EventKind::ConnectionGap,
        EventKind::BurstGap,
        EventKind::Reconnected,
    ];
    let mut history = History {
        events: kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| HistoryEvent {
                timestamp: u64::try_from(index + 1).unwrap(),
                kind,
            })
            .collect(),
    };
    assert_eq!(
        History::from_setting(&history.encode()).events,
        history.events
    );
    let export = history.export_json();
    for event in &history.events {
        assert!(export.contains(event.kind.export_name()));
        assert_ne!(event.kind.label(), "");
    }

    for target in [
        crate::model::Target::Allow,
        crate::model::Target::Block,
        crate::model::Target::Reject,
        crate::model::Target::Other(99),
    ] {
        history.record_target(target);
    }

    let file = gio::File::for_path(std::env::temp_dir().join(format!(
        "usbguard-gui-history-test-{}.json",
        std::process::id()
    )));
    write_export(&file, &export);
    assert_eq!(
        file.load_contents(None::<&gio::Cancellable>)
            .unwrap()
            .0
            .as_ref(),
        export.as_bytes()
    );
    let _ = file.delete(None::<&gio::Cancellable>);

    let file = gio::File::for_path(std::env::temp_dir().join(format!(
        "usbguard-gui-history-result-{}.json",
        std::process::id()
    )));
    finish_export(Ok(file.clone()), &export);
    assert!(file.query_exists(None::<&gio::Cancellable>));
    let _ = file.delete(None::<&gio::Cancellable>);
    finish_export(
        Err(glib::Error::new(gio::IOErrorEnum::Cancelled, "cancelled")),
        &export,
    );

    let directory = gio::File::for_path(std::env::temp_dir());
    write_export(&directory, &export);
}

#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise(app: &adw::Application) {
    let parent = adw::ApplicationWindow::builder().application(app).build();
    let kinds = [
        EventKind::Inserted,
        EventKind::Removed,
        EventKind::Authorized,
        EventKind::Blocked,
        EventKind::Rejected,
        EventKind::ActionAllowed,
        EventKind::ActionBlocked,
        EventKind::ActionRejected,
        EventKind::ActionFailed,
        EventKind::ConnectionGap,
        EventKind::BurstGap,
        EventKind::Reconnected,
    ];
    let history = History {
        events: kinds
            .into_iter()
            .enumerate()
            .map(|(index, kind)| HistoryEvent {
                timestamp: if index == 10 {
                    u64::MAX
                } else {
                    u64::try_from(index + 1).unwrap()
                },
                kind,
            })
            .collect(),
    };
    let cancelled = gio::Cancellable::new();
    cancelled.cancel();
    history.show_with_cancellable(app, &parent, Some(cancelled.clone()));
    super::super::tests::pump();
    let shown = app
        .windows()
        .into_iter()
        .find(|window| window.title().as_deref() == Some("Event history"))
        .unwrap();
    assert!(
        super::super::tests::widgets::<gtk::Label>(&shown)
            .iter()
            .any(|label| label.label().contains("Observation gap"))
    );
    super::super::tests::button(&shown, "Export privacy-safe JSON…").emit_clicked();
    super::super::tests::pump();
    save_export(&shown, history.export_json(), Some(&cancelled));
    super::super::tests::pump();
    shown.close();
    parent.close();
    super::super::tests::pump();
}
