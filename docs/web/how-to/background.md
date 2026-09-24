# Keep announcing devices with the window closed

When a device that is not authorized is plugged in, USBGuardGUI announces it — as a toast if the
window is in front, otherwise as a desktop notification with an **Allow for this session** button.
The button re-checks that the device is still the one announced before acting, since USBGuard
reuses device numbers.

To keep announcing after the window is closed: **Main menu → Preferences**, and turn on:

- **Keep running in the background** — closing the window hides it instead of quitting;
- **Start at login** — start hidden when you log in, so the first device of the session is
  announced too.

## With and without a status icon

Where the desktop provides a status icon area (KDE Plasma, Xfce, Cinnamon, and others), an icon
appears while the program runs in the background: click it to reopen the window, or use its menu
to quit.

**Stock GNOME has no status icon area** — it needs the *AppIndicator and KStatusNotifierItem
Support* extension. Without it, the program still runs and still announces devices; the first time
you close the window it tells you so. To reopen the window, launch USBGuard again from the
application menu; to quit, use **Main menu → Quit** (or <kbd>Ctrl</kbd>+<kbd>Q</kbd>).
