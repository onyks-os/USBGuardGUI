<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# Dependency Policy and Reference

This document is the authoritative inventory of every dependency USBGuardGUI relies on, and the
policies governing how dependencies are selected, pinned, monitored, and upgraded.

---

## 1. Dependency Directory

> **No crate has been added to `Cargo.toml` yet.** The tables below record the *choice* of
> dependency and the reason for it, which is the part that is decided. Exact versions are resolved
> with `cargo add` when each crate is first needed, and are then pinned by the committed
> `Cargo.lock` — transcribing them here would go stale silently. The **License** column states the
> license the crate is expected to carry; it is confirmed against the crate's own metadata by
> `cargo deny check licenses` in CI at the moment the crate is added, and corrected here if it
> differs.

### 1.1 Runtime Dependencies

| Package | Version constraint | License | Purpose | Security justification |
| :------ | :----------------- | :------ | :------ | :--------------------- |
| `gtk4` | Resolved at add time; feature-gated to GTK 4.14 | MIT | GTK 4 bindings — every widget in the interface. | The canonical Rust binding, maintained by the GNOME Rust team. A hand-rolled FFI layer to GTK would be a far larger unsafe surface than the binding it replaced. |
| `libadwaita` | Resolved at add time; ≥ 1.5 | MIT | Adaptive layout, `AdwToastOverlay`, `AdwAlertDialog`, `AdwApplication`. | Same maintainers as `gtk4`; avoids reimplementing the platform's dialog and toast semantics, which is where inconsistent confirmation UX would otherwise creep in. |
| `zbus` | Resolved at add time | MIT | D-Bus client, and the `#[proxy]` macro that generates the typed proxies. | Pure Rust, no `libdbus` FFI. The proxy macro keeps raw D-Bus signatures in exactly one file, which is what makes the boundary auditable. |
| `tokio` | Resolved at add time; features `rt-multi-thread`, `time`, `macros` | MIT | The I/O runtime that owns every D-Bus interaction. | Only the features listed are enabled; `net`, `fs`, and `process` are deliberately absent, so the dependency cannot open a socket or touch the filesystem on the program's behalf. |
| `async-channel` | Resolved at add time | Apache-2.0 OR MIT | The executor-agnostic bridge between the Tokio runtime and the GLib main loop. | Executor-agnostic by design, which is the whole requirement: a Tokio-specific channel would couple the two loops the architecture keeps apart. |
| `futures-util` | Resolved at add time | MIT OR Apache-2.0 | Stream combinators for the event worker's coalescing window. | Ecosystem-standard, minimal transitive tree. |
| `notify-rust` | Resolved at add time | MIT OR Apache-2.0 | Desktop notifications over the session bus. | Wraps a session-bus protocol that is fiddly to get right (capabilities negotiation, action callbacks). Optional at runtime: see §1.4. |
| `ksni` | Resolved at add time | Apache-2.0 | `StatusNotifierItem` tray icon. | The only maintained Rust implementation of the SNI protocol. Optional at runtime: see §1.4. |
| `tracing` | Resolved at add time | MIT | Structured logging and the spans that measure the latency targets. | Span-based rather than line-based, which is what makes the identifier-redaction policy enforceable per target instead of per call site. |
| `tracing-subscriber` | Resolved at add time | MIT | `EnvFilter`, driven by `USBGUARD_GUI_LOG`. | Same. |
| `thiserror` | Resolved at add time | MIT OR Apache-2.0 | Derives the error taxonomy that crosses layers. | Compile-time only in effect; adds no runtime behaviour to get wrong. |
| `gettext-rs` | Resolved at add time | MIT | Translation catalogue lookup. | The platform's own i18n mechanism, so translations integrate with the distribution's tooling rather than a bespoke format. |

### 1.2 Development Dependencies

| Package | Version constraint | License | Purpose |
| :------ | :----------------- | :------ | :------ |
| `proptest` | Resolved at add time | MIT OR Apache-2.0 | Property-based round-trip tests for the rule parser and renderer. |
| `cargo-fuzz` / `libfuzzer-sys` | Toolchain, not a manifest entry | MIT OR Apache-2.0 | Fuzzing the parser; target of zero panics over 10⁶ inputs. |
| `cargo-llvm-cov` | Toolchain | MIT OR Apache-2.0 | Coverage measurement against the >90% parser target. |
| `cargo-audit` | Toolchain | MIT OR Apache-2.0 | `make audit` — RustSec advisory scanning. |
| `cargo-deny` | Toolchain | MIT OR Apache-2.0 | License and duplicate-dependency checks; the authority for the License columns here. |
| Clippy, rustfmt | Toolchain component | MIT OR Apache-2.0 | `make lint`; Clippy carries the `disallowed-methods` lint that enforces the no-panic and runtime-boundary rules. |
| `cargo-deb`, `cargo-generate-rpm` | Toolchain | MIT | Native package building. |

### 1.3 System-Level Dependencies

| Package | Minimum version | Provided by | Purpose |
| :------ | :-------------- | :---------- | :------ |
| `usbguard` (daemon) | 1.1.0 | Distribution | The daemon this program is a client of. Earlier versions have a different `appendRule` arity. **Hard dependency** in native packages. |
| `usbguard-dbus` (bridge) | Matching the daemon | Distribution — a *separate* package everywhere | The D-Bus bridge exposing `org.usbguard1`. Without it the program cannot function at all. **Hard dependency** in native packages; its absence is a first-class diagnostic state. |
| GTK | 4.14 | Distribution | Runtime and build-time (`gtk4-devel`). |
| libadwaita | 1.5 | Distribution | Runtime and build-time (`libadwaita-devel`). |
| GLib | Matching GTK | Distribution | Main loop, GSettings, GResource (`glib2-devel`). |
| `pkg-config` | any | Distribution | Build-time discovery of the above. |
| A C toolchain | any | Distribution | Required by the `-sys` crates underlying the GTK bindings. |
| D-Bus (system bus) | any | Distribution | The only transport to the daemon. |

On Fedora: `gtk4-devel libadwaita-devel glib2-devel pkgconf-pkg-config gcc`.

### 1.4 Optional & Dynamic Dependencies

Every entry here is a *runtime* service that may or may not exist in the user's session. None is a
build-time option, and the absence of any of them is a supported configuration that degrades
visibly rather than silently.

| Package | When required | Degradation if absent |
| :------ | :------------ | :-------------------- |
| A Polkit session agent | Whenever an operation needs authorization | Denials arrive with no prompt, which is indistinguishable from a policy refusal unless detected. The probe sequence records agent presence specifically so this can be explained rather than guessed at. |
| A notification server (`org.freedesktop.Notifications`) | Insertion notifications | No notifications. The setting is left enabled; the feature simply has nowhere to go. |
| A `StatusNotifierWatcher` (`org.kde.StatusNotifierWatcher`) | Tray icon | Background mode, with a one-time visible notice. Never a silent no-op — a tray icon the user cannot see is worse than an explicit "your desktop does not provide a tray". |
| Notification server action support | Quick actions in notifications | A plain notification that opens the window instead of acting inline. |

---

## 2. Dependency Management Policies

### 2.1 Dependency Vetting and Selection

Before a new dependency is added, it must satisfy all of the following:

1. **Necessity**: the functionality cannot be implemented in a reasonable amount of first-party code.
2. **Maintenance**: the project has had a release or substantive commit within the last 12 months.
3. **License compatibility**: the license is compatible with MIT (see [SCA_POLICY.md](SCA_POLICY.md)).
4. **Transitive weight**: the dependency does not pull in a disproportionate transitive tree.
5. **Provenance**: the package is published by its upstream maintainers, not a third-party mirror.

The rationale for each accepted dependency is recorded in the tables above.

### 2.2 Version Pinning & Range Rules

- **Applications**: dependencies are pinned to exact versions in the lockfile, which is committed.
- **Libraries**: dependencies declare a minimum version and an upper bound at the next major.
- **CI actions**: pinned to a full commit SHA.

### 2.3 Vulnerability Monitoring & Remediation

- Dependabot monitors the manifest and opens update pull requests automatically.
- `make audit` runs the ecosystem's vulnerability scanner locally and in CI.
- Remediation timelines are defined in [SCA_POLICY.md](SCA_POLICY.md).

### 2.4 Upgrading Dependencies

1. Read the upstream changelog for breaking changes.
2. Run `make verify` against the upgrade.
3. Record the upgrade in [`CHANGELOG.md`](CHANGELOG.md) and update the tables above.

### 2.5 Software Bill of Materials (SBOM)

A CycloneDX SBOM is generated and attached to every release; see
[`.github/workflows/release.yml`](.github/workflows/release.yml) and
[`docs/verification.md`](docs/verification.md).
