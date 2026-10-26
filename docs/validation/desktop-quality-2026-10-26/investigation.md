# Desktop investigation and final validation — 2026-10-26

The final diagnostic executable passed both native desktop suites. The history
below distinguishes the application fix from a test-observer correction; the
linked receipts are not installed-package release validation.

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
At that checkpoint, the driver error prevented a valid comparison of baseline
and experimental background-close timing. The follow-up below records the
controlled comparison and exact guest version.

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
The GTK contract passes with fatal criticals. The corrected diagnostic executable
also passed all seven GNOME cases, including ten hide/relaunch cycles with fatal
criticals and no debugger; see [the exact receipt](gnome.json). It is not an
installed-package receipt. No upstream issue or fix is claimed.

The Plasma run passed preferences and six stress cycles, then found a different
critical: `g_variant_is_object_path: assertion string != NULL failed`. This required
a separate investigation, resolved for the observer below. The
initial Plasma watcher startup also needed a longer property-call deadline under
TCG, before any application had launched. That startup timeout is distinct from
the later primary-GUI critical.


## AT-SPI observer follow-up

The original failed PID's core dump and a later live GDB reproduction both
enter GTK's `handle_accessible_method`, then `g_variant_new_object_path` with a
null path. The immediate caller is GTK's accessibility D-Bus handler, rather
than an application window callback:

```text
g_variant_is_object_path
g_variant_new_object_path
g_variant_new
handle_accessible_method.part.0       (libgtk-4.so.1)
call_in_idle_cb                      (libgio-2.0.so.0)
g_idle_dispatch
```

This matches the stale-index path in [GTK 4.22.5's root handler](https://github.com/GNOME/gtk/blob/4.22.5/gtk/a11y/gtkatspiroot.c):
the fixture read a child count and requested children separately while the
window was hiding. That root handler can still try to return the last hidden
window's unrealized accessibility path. The fixture now observes the target
application's children atomically with `GetChildren`, using the exact app's
AT-SPI bus name/path, and traverses its widget tree only when a window is
present. It keeps both fatal criticals and the final critical-message gate.
This changes the observer, not GTK, and does not claim to fix GTK for other
clients using stale child indices.

Final runs with this same observer and executable passed without a debugger:

| Desktop | Functional cases | Filter/hide/relaunch cycles | Receipt |
| :--- | :--- | :--- | :--- |
| GNOME Shell 50.6 | 7/7 | 10 | [GNOME](gnome.json) |
| KWin/Plasma 6.7.5 | 7/7 | 30 | [Plasma](kde.json) |

Both kept fatal criticals enabled and verified clean exit, the same application
owner across cycles, and a single background notification. The Plasma run used
its real watcher, tray activation and Quit menu; GNOME exercised no-watcher
behavior. Binary, driver, helper and protocol hashes were checked when collecting
the receipts. Whole-application coverage is [5500/5607 lines](coverage.json),
98.0917%. The private VM was then shut down.
