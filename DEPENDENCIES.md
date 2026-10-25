<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# Dependency Policy and Reference

This document is the authoritative inventory of every dependency USBGuardGUI relies on, and the
policies governing how dependencies are selected, pinned, monitored, and upgraded.

---

## 1. Dependency Directory

> Every runtime crate below is in use; `notify-rust` was considered and not added. The tables
> record the *choice* of dependency and the reason for it; exact versions are pinned by the
> committed `Cargo.lock` — major versions are noted where they matter (`gtk4` 0.10 and
> `libadwaita` 0.8 must move together). The **License** column was checked against each crate's
> own metadata (`cargo metadata`) and is enforced across the root manifest's full
> dependency graph by `make dependency-policy` and the CI license/source job.

### 1.1 Runtime Dependencies

| Package | Version constraint | License | Purpose | Security justification |
| :------ | :----------------- | :------ | :------ | :--------------------- |
| `gtk4` | 0.10, feature `v4_14`; optional, behind the Cargo feature `gui` | MIT | GTK 4 bindings — every widget in the interface. | The canonical Rust binding, maintained by the GNOME Rust team. A hand-rolled FFI layer to GTK would be a far larger unsafe surface than the binding it replaced. |
| `libadwaita` | 0.8, feature `v1_5`; optional, behind `gui` | MIT | Adaptive layout, `AdwToastOverlay`, `AdwAlertDialog`, `AdwApplication`. | Same maintainers as `gtk4`; avoids reimplementing the platform's dialog and toast semantics, which is where inconsistent confirmation UX would otherwise creep in. |
| `zbus` | 6.1, default features off, features `tokio`, `proxy`; `service` behind `gui` | MIT | D-Bus client, and the `#[proxy]` macro that generates the typed proxies. | Pure Rust, no `libdbus` FFI. The proxy macro keeps raw D-Bus signatures in exactly one file, which is what makes the boundary auditable. |
| `tokio` | 1.53.3, features `rt-multi-thread`, `time`, `macros`, `sync` | MIT | The I/O runtime that owns every D-Bus interaction. | This crate enables only the features listed. `zbus`'s `tokio` backend additionally enables `net`, `fs`, and `process` (verified with `cargo tree -e features -i tokio`): it needs `net` for the bus socket, and the others for address discovery and authentication. The earlier claim that the dependency *cannot* open a socket or touch the filesystem was therefore wrong; the guarantee comes from the program's own code, verified by the `strace` target of `docs/architecture.md` §13.2, not from feature flags. |
| `async-channel` | 2 | Apache-2.0 OR MIT | The executor-agnostic bridge between the Tokio runtime and the GLib main loop. | Executor-agnostic by design, which is the whole requirement: a Tokio-specific channel would couple the two loops the architecture keeps apart. |
| `futures-util` | 0.3 | MIT OR Apache-2.0 | Stream combinators for the event worker's coalescing window. | Ecosystem-standard, minimal transitive tree. |
| ~~`notify-rust`~~ | not used | — | Replaced by GIO's `GNotification`, already part of the GTK stack. | GIO speaks both the session-bus protocol and the Flatpak notification portal, and routes button presses to application actions. One dependency fewer; capabilities are still queried (`GetCapabilities`) before buttons are offered. |
| `tracing` | 0.1 | MIT | Structured logging and the spans that measure the latency targets. | Span-based rather than line-based, which is what makes the identifier-redaction policy enforceable per target instead of per call site. |
| `tracing-subscriber` | 0.3, feature `env-filter` | MIT | `EnvFilter`, driven by `USBGUARD_GUI_LOG`. | Same. |
| `serde` | 1, feature `derive`; optional, behind `gui` | MIT OR Apache-2.0 | Serialize the standard D-Bus menu layout. | Used by the fixed first-party SNI/menu interfaces on zbus, without a second protocol dependency. |
| `thiserror` | 2 | MIT OR Apache-2.0 | Derives the error taxonomy that crosses layers. | Compile-time only in effect; adds no runtime behaviour to get wrong. |
| `gettext-rs` | 0.8, default features off, feature `gettext-system`; optional, behind `gui` | MIT | Translation catalogue lookup. | The platform's own i18n mechanism, so translations integrate with the distribution's tooling rather than a bespoke format. |

### 1.2 Development Dependencies

Reproducibility tooling additionally uses Python 3.11+ and rootless Podman (or
Docker). Its compiler and native build inputs are pinned by
[`packaging/reproducible/Containerfile`](packaging/reproducible/Containerfile),
[`rust-toolchain.toml`](packaging/reproducible/rust-toolchain.toml), and the Rust
archive checksum: Ubuntu 24.04 digest, snapshot `20261001T000000Z`, Rust 1.89.0.
The snapshot also pins native packaging tools: RPM, dpkg, makepkg/pacman,
fakeroot, libarchive, zstd, Flatpak, OSTree, SVG validation, gettext, cpio and Python.
The reproducibility unit suite additionally requires PyYAML.
The compiler archive and bootstrap Ubuntu CA package are SHA-256 checked. This
recipe pin does not change runtime package requirements or the developer/MSRV
matrix. See [verification](docs/verification.md#5-reproducing-the-build).

Private-bus integration tests require `dbus-daemon`. The separate graphical
regression test requires a display; CI provides it with `xvfb` and `xauth`.
These are test tools, not additional package/runtime dependencies.

| Package | Version constraint | License | Purpose |
| :------ | :----------------- | :------ | :------ |
| `tokio` (test features) | 1.53, adds `net`, `rt-multi-thread`, `macros`, `time`, `test-util` | MIT | Private mock D-Bus sockets and virtual time for ten-minute authentication waits. `test-util` is development-only; production builds do not enable it. |
| `proptest` | 1 | MIT OR Apache-2.0 | Property-based round-trip tests for the rule parser and renderer. |
| `cargo-fuzz` / `libfuzzer-sys` | Toolchain, not a manifest entry | MIT OR Apache-2.0 | Fuzzing the parser; target of zero panics over 10⁶ inputs. |
| `cargo-llvm-cov` | Toolchain; CI pins 0.9.1 | MIT OR Apache-2.0 | `make coverage-core` enforces >90% line coverage per rule-language/operation-core file; `make coverage` produces an all-feature HTML report. |
| `cargo-audit` | Toolchain | MIT OR Apache-2.0 | `make audit` — RustSec advisory scanning. |
| `cargo-deny` | Toolchain; CI pins 0.20.2 | MIT OR Apache-2.0 | Enforces the explicit license/source policy in `deny.toml`, including root-manifest optional, build, test and target-specific dependencies. Vulnerabilities remain the responsibility of cargo-audit; duplicate versions are not globally forbidden. |
| Clippy, rustfmt | Toolchain component | MIT OR Apache-2.0 | `make lint`; Clippy enforces the no-panic rules (lint levels in `Cargo.toml`, denied outright on `src/rules/`) and, through `disallowed-methods`, the runtime-boundary rule. |
| `cargo-deb`, `cargo-generate-rpm` | Toolchain | MIT | Native package building. |

### 1.3 System-Level Dependencies

| Package | Minimum version | Provided by | Purpose |
| :------ | :-------------- | :---------- | :------ |
| `usbguard` (daemon) | 1.1.0 | Distribution | The daemon this program is a client of. Earlier versions have a different `appendRule` arity. **Hard dependency** in native packages. |
| `usbguard-dbus` (bridge) | Matching the daemon | Distribution — inside `usbguard` on Debian, Ubuntu, and Arch; a separate `usbguard-dbus` package on Fedora | The D-Bus bridge exposing `org.usbguard1`. Without it the program cannot function at all. **Hard dependency** of the `.rpm`; its absence is a first-class diagnostic state. |
| GTK | 4.14 | Distribution | Runtime and build-time (`gtk4-devel`). |
| libadwaita | 1.5 | Distribution | Runtime and build-time (`libadwaita-devel`). |
| GLib | Matching GTK | Distribution | Main loop, GSettings, GResource (`glib2-devel`). |
| GNU gettext | any | Distribution | Build time only: `msgfmt` compiles the translations. Without it the build succeeds in English only, with a warning. |
| `pkg-config` | any | Distribution | Build-time discovery of the above. |
| A C toolchain | any | Distribution | Required by the `-sys` crates underlying the GTK bindings. |
| D-Bus (system bus) | any | Distribution | The only transport to the daemon. |

- Fedora / RHEL: `gtk4-devel libadwaita-devel glib2-devel pkgconf-pkg-config gcc`
- Debian / Ubuntu (24.04+): `libgtk-4-dev libadwaita-1-dev pkg-config build-essential`
- Arch: `gtk4 libadwaita pkgconf base-devel`

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

`make dependency-policy` checks the committed root lockfile with all features and
without a target filter. `deny.toml` permits only the explicitly reviewed SPDX
choices used by this graph: MIT, Apache-2.0, Apache-2.0 WITH LLVM-exception,
Unicode-3.0 and Unlicense. Other choices and unknown licenses fail. For an OR
expression an allowed choice is sufficient; every part of an AND expression
must be allowed. A new license requires a policy review rather than an automatic
allow-list expansion. The system-library licenses remain separate from this
Rust crate check.

Sources are restricted to crates.io: unapproved Git and alternate registries
fail. A future Git exception must identify an explicitly reviewed URL and full
revision. `make test-dependency-policy` exercises real disposable Cargo graphs,
including a forbidden Git source pinned to a commit. It needs Git, Python 3,
Cargo and the pinned cargo-deny, and does not compile or fetch external fixtures.
The separate fuzz workspace/tool installations are outside the root lockfile.

### 2.5 Software Bill of Materials (SBOM)

A CycloneDX SBOM is generated and attached to every release; see
[`.github/workflows/release.yml`](.github/workflows/release.yml) and
[`docs/verification.md`](docs/verification.md).

## Audit follow-up (2026-10-24)

The 0.1.2 release raises Tokio's minimum to 1.53.3 and updates the lockfile to
resolve [RUSTSEC-2026-0386](https://rustsec.org/advisories/RUSTSEC-2026-0386.html),
which concerns the I/O-safety contract of AsyncFd constructors. No application
unsafe code was added.

RustSec also reports informational unmaintained advisories RUSTSEC-2026-0370,
RUSTSEC-2026-0371 and RUSTSEC-2026-0372 for zvariant, zbus_names and
zvariant_derive. [Upstream merged these into zbus 6](https://github.com/z-galaxy/zbus/issues/2043).
They remain transitive dependencies of the tested zbus 5/ksni stack in 0.1.2;
the advisories are recorded, not globally suppressed. A coordinated zbus/ksni
migration needs its own D-Bus compatibility, MSRV and tray tests rather than an
unchecked major dependency change during this patch release.

The post-0.1.2 migration uses zbus 6.1 with a Rust 1.89 minimum and the
same pinned Rust 1.89.0 reproducible builder. Published ksni 0.3.7 still uses
zbus 5, so the tray now implements its fixed StatusNotifierItem and D-Bus menu
interfaces directly on zbus, the alternative already described in architecture
§9.6. There is no Git dependency or upstream fork. The deprecated zvariant,
zvariant_derive and zbus_names crates are absent from the lockfile.

Private-bus wire tests cover native and Flatpak unique-name registration, menu
layouts/actions, tooltip changes, watcher restart, failure without a host and
shutdown. The USBGuard mock tests retain interactive-authorization flags,
transport-loss classification and reconnection checks with zbus 6. Flatpak
requires xdg-dbus-proxy 0.1.6 or newer, as required by zbus 6.
