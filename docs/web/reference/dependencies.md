# Reference: Dependencies

Exhaustive inventory of runtime, development, and system dependencies.

This page mirrors
[`DEPENDENCIES.md`](https://github.com/onyks-os/USBGuardGUI/blob/main/DEPENDENCIES.md),
which is the authoritative source and also records the license and security justification of each
entry.

!!! note "No crate has been added yet"
    `Cargo.toml` declares no runtime dependency. The table below records the crate *chosen* for each
    job; each one is added with `cargo add` at the start of the roadmap phase that first needs it,
    and its exact version is then pinned by the committed `Cargo.lock`.

---

## 1. Runtime Dependencies

| Package | Version | License | Purpose |
| :------ | :------ | :------ | :------ |
| `gtk4` | Resolved at add time | MIT | GTK 4 bindings; feature-gated to GTK 4.14. |
| `libadwaita` | Resolved at add time | MIT | Adwaita widgets, adaptive layout, toasts, alert dialogs. |
| `zbus` | Resolved at add time | MIT | D-Bus client and the `#[proxy]` macro. |
| `tokio` | Resolved at add time | MIT | I/O runtime; features `rt-multi-thread`, `time`, `macros` only. |
| `async-channel` | Resolved at add time | Apache-2.0 OR MIT | Executor-agnostic bridge between the Tokio runtime and the GLib main loop. |
| `futures-util` | Resolved at add time | MIT OR Apache-2.0 | Stream combinators for event coalescing. |
| `notify-rust` | Resolved at add time | MIT OR Apache-2.0 | Desktop notifications. |
| `ksni` | Resolved at add time | Apache-2.0 | `StatusNotifierItem` tray icon. |
| `tracing`, `tracing-subscriber` | Resolved at add time | MIT | Structured logging and latency spans. |
| `thiserror` | Resolved at add time | MIT OR Apache-2.0 | The error taxonomy that crosses layers. |
| `gettext-rs` | Resolved at add time | MIT | Translation catalogues. |

## 2. Development Dependencies

| Package | Version | Purpose |
| :------ | :------ | :------ |
| `proptest` | `1` | Round-trip property tests for the rule parser and renderer. |
| `cargo-fuzz` / `libfuzzer-sys` | Toolchain | Parser fuzzing; target of zero panics over 10⁶ inputs. |
| `cargo-llvm-cov` | Toolchain | Coverage, against the >90% parser target. |
| `cargo-audit` | Toolchain | RustSec advisory scanning (`make audit`). |
| `cargo-deny` | Toolchain | License and duplicate-dependency checks. |
| Clippy, rustfmt | Toolchain | `make lint`; Clippy enforces the no-panic and runtime-boundary rules. |
| `cargo-deb`, `cargo-generate-rpm` | Toolchain | Native package building. |

## 3. System Dependencies

| Binary | Minimum version | Purpose |
| :----- | :-------------- | :------ |
| `usbguard-daemon` | 1.1.0 | The daemon this program is a client of. Earlier versions differ in `appendRule` arity. |
| `usbguard-dbus` | Matching the daemon | The D-Bus bridge. A **separate package** on every reference distribution; without it the program cannot function. |
| GTK | 4.14 | Widgets. Build-time: `gtk4-devel`. |
| libadwaita | 1.5 | Adaptive layout. Build-time: `libadwaita-devel`. |
| GLib | Matching GTK | Main loop, GSettings, GResource. Build-time: `glib2-devel`. |
| `pkg-config` | any | Build-time discovery of the above. |
| A C toolchain | any | Required by the `-sys` crates under the GTK bindings. |
| D-Bus (system bus) | any | The only transport to the daemon. |

Optional at runtime, each degrading visibly rather than silently: a Polkit session agent, a
notification server, and a `StatusNotifierWatcher`.
