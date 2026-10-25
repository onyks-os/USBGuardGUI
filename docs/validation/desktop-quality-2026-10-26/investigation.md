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
