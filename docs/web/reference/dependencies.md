# Reference: Dependencies

Exhaustive inventory of runtime, development, and system dependencies.

This page mirrors
[`DEPENDENCIES.md`](https://github.com/onyks-os/USBGuardGUI/blob/main/DEPENDENCIES.md),
which is the authoritative source and also records the security justification of each entry. Exact
versions are pinned by the committed `Cargo.lock`.

---

## 1. Runtime Dependencies

| Package | Version | License | Purpose |
| :------ | :------ | :------ | :------ |
| `gtk4` | 0.10, feature `v4_14` | MIT | GTK 4 bindings. Optional: Cargo feature `gui`. |
| `libadwaita` | 0.8, feature `v1_5` | MIT | Adwaita widgets, toasts, dialogs. Optional: `gui`. |
| `ksni` | 0.3, feature `tokio` | Unlicense | `StatusNotifierItem` tray icon, built on zbus. Optional: `gui`. |
| `zbus` | 5, feature `tokio` | MIT | D-Bus client and the `#[proxy]` macro. |
| `tokio` | 1 | MIT | The I/O runtime that owns every D-Bus call. |
| `async-channel` | 2 | Apache-2.0 OR MIT | Executor-agnostic bridge between Tokio and the GLib main loop. |
| `futures-util` | 0.3 | MIT OR Apache-2.0 | Stream combinators for event coalescing. |
| `tracing`, `tracing-subscriber` | 0.1, 0.3 | MIT | Structured logging, driven by `USBGUARD_GUI_LOG`. |
| `thiserror` | 2 | MIT OR Apache-2.0 | The error taxonomy that crosses layers. |

Desktop notifications use GIO's own `GNotification`, which already speaks both the session-bus
protocol and the Flatpak notification portal, so no notification crate is needed. Translations
(`gettext-rs`) are planned.

`cargo build --no-default-features` builds the headless commands (`--diagnose`, `--list-devices`,
`--list-rules`) without GTK.

## 2. Development Dependencies

| Package | Version | Purpose |
| :------ | :------ | :------ |
| `proptest` | 1 | Property tests for the rule parser and renderer. |
| `zbus` feature `p2p` | 5 | The mock USBGuard bridge the D-Bus tests run against. |
| `cargo-fuzz` / `libfuzzer-sys` | Toolchain (nightly) | Parser fuzzing: `make fuzz-parser`. |
| `cargo-audit` | Toolchain | RustSec advisory scanning (`make audit`). |
| Clippy, rustfmt | Toolchain | `make lint`; Clippy enforces the no-panic and runtime-boundary rules. |
| `cargo-deb`, `cargo-generate-rpm` | Toolchain | `make package-deb`, `make package-rpm`. |

## 3. System Dependencies

| Component | Minimum version | Purpose |
| :-------- | :-------------- | :------ |
| `usbguard` | 1.1.0 | The daemon this program is a client of. Earlier versions differ in `appendRule` arity. |
| USBGuard D-Bus bridge | Matching the daemon | Provides `org.usbguard1`. Inside `usbguard` on Debian, Ubuntu, and Arch; the separate `usbguard-dbus` package on Fedora and RHEL. |
| GTK | 4.14 | Widgets. Build: `gtk4-devel` / `libgtk-4-dev` / `gtk4`. |
| libadwaita | 1.5 | Adwaita widgets. Build: `libadwaita-devel` / `libadwaita-1-dev` / `libadwaita`. |
| GLib | Matching GTK | Main loop, GSettings. `glib-compile-schemas` compiles the settings schema at build time. |
| A C toolchain, `pkg-config` | any | Required by the `-sys` crates under the GTK bindings. |
| D-Bus (system bus) | any | The only transport to the daemon. |

Optional at runtime, each degrading visibly rather than silently: a Polkit session agent, a
notification server, and a `StatusNotifierWatcher`.
