# Desktop investigation checkpoint — 2026-10-26

This is unfinished investigation evidence, not a passing release receipt.

Private native Wayland sessions in the marked synthetic Fedora guest passed six
functional cases each on GNOME Shell 50.6 and KWin/Plasma 6.7.5. A GNOME run also
emitted an intermittent critical:

```text
GLib-GObject-CRITICAL: g_object_notify_by_pspec: assertion 'G_IS_OBJECT (object)' failed
```

A subsequent GDB run with fatal criticals stopped on the GUI main thread:

```text
_g_log_abort
 g_logv
 g_log
 signal_emit_valist_unlocked
 g_signal_emit_valist
 g_signal_emit
 button_activate_timeout                 (libgtk-4.so.1)
 g_timeout_dispatch
 g_main_context_dispatch_unlocked
 g_main_context_iteration
 g_application_run
```

The attempted repeated-cycle driver also invoked AT-SPI `GrabFocus`, which is
unsupported by the inspected [GTK 4.22.5 component implementation](https://github.com/GNOME/gtk/blob/4.22.5/gtk/a11y/gtkatspicomponent.c).
That driver error must be removed before comparing baseline and experimental
background-close timing. Neither the proposed deferral nor the ten planned
cycles has been validated. No upstream or first-party cause is established.
The exact guest GTK version still needs recording.

Raw diagnostics and preliminary six-case reports remain under ignored `target/`;
SESSION_HANDOFF.md records their paths and the exact candidate identity.

## Controlled follow-up

The guest package is `gtk4-4.22.5-2.fc44.x86_64`. Removing the unsupported
GrabFocus call made it possible to compare the same formal test against the
installed original (`71b56efffc9d97c6ad9228d8ae03356453c24a40f604d7e6977d6e27ca9fbbbd`)
and the deferred-background-close experiment
(`b2c9367f58969136a8738194c456ad4711426dc197c67e56f97d8f28dba55a30`).
Both stopped with the same GObject critical with fatal criticals enabled. The
ineffective background-close experiment was therefore reverted.

A breakpoint immediately before each `button_activate_timeout` identified the
first affected control as `GtkButton`, with parent `AdwSheetControls`; it was
the native preferences-dialog Close button. GDB catches the fatal trap and
can then let later GUI steps continue after detaching. Consequently, later
printed hide/relaunch progress does not locate the original critical and is
not clean acceptance evidence.

A separate minimal GTK/libadwaita program reproduced the critical on an isolated
X11 display too: with animations disabled, drop external references to the
preferences dialog, then activate its native Close button. Holding the dialog
through the callback prevents the critical. References held accidentally by a
test can therefore conceal this lifetime edge case. A Wayland minimal-program
attempt used a timing assumption too short for TCG; that failed timer assertion
does not establish a second application defect.

The new application candidate retains each transient dialog when `closed` is
emitted and releases that reference at the next GLib idle iteration. This
applies to preferences, About, diagnostics, rule editor and alerts. Native
Close-button contracts cover preferences/About without keeping test references
alive during the callback, and verify that the dialogs are subsequently freed.
The GTK contract passes with fatal criticals; final GNOME/Plasma acceptance on
the corrected executable is still pending. No upstream issue or fix is claimed.
