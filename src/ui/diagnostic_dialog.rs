//! The diagnostic panel (docs/architecture.md §4, §9.1): a dialog, not a
//! page. It re-runs the probe when opened, shows what each check found, and
//! the specific remedy — with a copy button, never a "fix it" button: every
//! remedy changes system configuration, which is the administrator's call.

use adw::prelude::*;
use gtk::glib;

use crate::cli::format_report;
use crate::dbus::diagnostics;
use crate::remedy::{Distro, remedy};
use crate::runtime::runtime;
use gettextrs::gettext;

/// Opens the dialog over `parent`.
pub(super) fn present(parent: &impl IsA<gtk::Widget>) {
    let dialog = adw::AlertDialog::new(
        Some(&gettext("USBGuard access")),
        Some(&gettext("Checking…")),
    );
    dialog.add_response("close", &gettext("Close"));
    dialog.set_close_response("close");

    let report_label = gtk::Label::builder()
        .selectable(true)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["monospace"])
        .build();
    dialog.set_extra_child(Some(&report_label));
    dialog.present(Some(parent));

    // The probe runs on Tokio; the result comes back over a one-shot channel
    // and is applied here, on the main thread.
    let (tx, rx) = async_channel::bounded(1);
    runtime().spawn(async move {
        let _ = tx.send(diagnostics::probe().await).await;
    });
    glib::spawn_future_local(async move {
        let Ok(report) = rx.recv().await else {
            return;
        };
        let distro = Distro::detect();
        dialog.set_body(&super::i18n::access_summary(&report.state));
        report_label.set_text(&format_report(&report, distro));

        if let Some(remedy) = remedy(&report.state, distro).filter(|r| !r.commands.is_empty()) {
            let commands = remedy.commands.join("\n");
            dialog.add_response("copy", &gettext("Copy commands"));
            dialog.set_response_appearance("copy", adw::ResponseAppearance::Suggested);
            dialog.connect_response(Some("copy"), move |d, _| {
                d.clipboard().set_text(&commands);
            });
        }
    });
}
