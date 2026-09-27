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
///
/// A full dialog rather than an alert: the report is the `--diagnose` text,
/// aligned in columns, and an alert is too narrow to show it unwrapped.
pub(super) fn present(parent: &impl IsA<gtk::Widget>) {
    let copy = gtk::Button::builder()
        .label(gettext("Copy commands"))
        .css_classes(["suggested-action"])
        .visible(false)
        .build();
    let header = adw::HeaderBar::new();
    header.pack_start(&copy);

    let summary = gtk::Label::builder()
        .label(gettext("Checking…"))
        .xalign(0.0)
        .wrap(true)
        .css_classes(["title-4"])
        .build();
    let report_label = gtk::Label::builder()
        .selectable(true)
        .xalign(0.0)
        .yalign(0.0)
        .css_classes(["monospace"])
        .build();
    let report_frame = gtk::Frame::builder()
        .child(
            &gtk::ScrolledWindow::builder()
                .child(&report_label)
                .vscrollbar_policy(gtk::PolicyType::Never)
                .propagate_natural_height(true)
                .build(),
        )
        .build();
    report_label.set_margin_top(12);
    report_label.set_margin_bottom(12);
    report_label.set_margin_start(12);
    report_label.set_margin_end(12);

    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(6)
        .margin_bottom(18)
        .margin_start(18)
        .margin_end(18)
        .build();
    column.append(&summary);
    column.append(&report_frame);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(
        &gtk::ScrolledWindow::builder()
            .child(&column)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .build(),
    ));

    let dialog = adw::Dialog::builder()
        .title(gettext("USBGuard access"))
        .content_width(720)
        // Fixed: the report arrives after the dialog is sized, and a dialog
        // does not grow to fit it. Longer reports (with a remedy) scroll.
        .content_height(480)
        .child(&toolbar)
        .build();
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
        summary.set_label(&super::i18n::access_summary(&report.state));
        report_label.set_text(format_report(&report, distro).trim_end());
        // The selectable report is otherwise the first focusable widget, and
        // opens with a text cursor in it.
        dialog.set_focus(None::<&gtk::Widget>);

        if let Some(remedy) = remedy(&report.state, distro).filter(|r| !r.commands.is_empty()) {
            let commands = remedy.commands.join("\n");
            copy.connect_clicked(move |button| {
                button.clipboard().set_text(&commands);
            });
            copy.set_visible(true);
        }
    });
}
