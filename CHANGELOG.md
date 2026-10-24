<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- French translations for the interface, notifications, desktop entry and
  AppStream metadata; gettext/plural/format and package-catalogue tests, plus
  the graphical focus regression in both English and French.
- A pinned cargo-deny license/source gate in CI and `make verify`, with rejection
  tests for forbidden licenses across dependency kinds, optional/target-specific
  dependencies, SPDX AND/OR expressions and unapproved pinned Git sources.

### Changed

- Migrate to zbus 6.1 and Rust 1.89, including the CI minimum and pinned
  reproducible builder. Replace ksni with fixed SNI/D-Bus menu interfaces on
  zbus; preserve native/Flatpak registration, actions and watcher reconnection.
  The deprecated zvariant/zvariant_derive/zbus_names dependency stack is removed.
- Add disposable QEMU clean-install validation with synthetic USB storage,
  native packages and real Polkit/PAM; isolation guards are regression-tested.

### Fixed

- Installed Flatpak headless checks avoid starting the unrelated document portal.
- Reproducible builds compile every language registered in `po/LINGUAS`, and
  installed Flatpak validation checks every registered catalogue.

## [0.1.2] - 2026-10-24

### Added

- `make reproducible`: two clean offline Linux x86_64 GUI builds with a pinned Ubuntu
  image/snapshot and Rust 1.88.0. Byte comparison, temporary-path checks, GUI dependency
  inspection, source/recipe archives, and an environment report run locally and in CI.
- Reproduction inputs accompany new releases; the published bare binary comes only from the
  successful comparison job. Complete RPM, DEB, Arch and Flatpak archives are compared as well.
- Flatpak bundles use a dedicated /app-locale executable and fixed OSTree export clock;
  private runtime validation enforces a pinned GNOME commit and checks installed CLI/schema/assets.
- Unit tests for deterministic source archives, excluded credentials, recipe freezing,
  checker failure paths, and refusing stale outputs or unacknowledged local changes.
- Mock D-Bus resilience tests for interrupted writes before/after execution (T2), ten-minute
  authentication waits with virtual time (T13), and cancellation while authentication is pending.
- Regression tests for log privacy and startup notices when debug/trace is enabled.
- `make coverage-core` and a CI gate requiring >90% line coverage per rule-language,
  cancellable-operation, and error-classification file.
- A private-demo-bus filesystem tracing script and scoped measurement notes in `docs/testing.md`.

### Fixed

- Dependency-audit CI runs the pinned auditor directly with read-only permissions,
  avoiding failed check-run publication and keeping fork pull requests supported.
- Require Tokio 1.53.3 to resolve RUSTSEC-2026-0386 (AsyncFd I/O safety).
- Release builds no longer embed build-machine paths for development-only schemas/catalogues;
  installed system assets and debug builds retain their respective behavior.
- Device-table refreshes and reconnection resets clear the old row's keyboard focus before
  removing it. This avoids a GTK crash when focus returns from an authentication window after
  cancelling Polkit. A graphical regression test reproduces the original crash and runs in CI.
- Reconnection no longer spins when the bridge owns its bus name but cannot serve requests:
  retries observe exponential backoff, and reconnection is announced only after subscriptions
  and the first device read succeed. Tests now include a real private bus and name replacement.
- Writes whose replies are lost now report an inconclusive outcome and request fresh device/policy
  state without retrying automatically. The message is also translated into Italian.
- Connection and malformed-signal errors no longer include raw detail at default log levels;
  debug/trace logging prints a privacy notice, including with dependency-specific filters.

### Changed

- Documentation checked against 0.1.1. The security assessment now gives each mitigation the status
  the code actually supports, including four that are only partial; the roadmap, the site's
  quickstart, the dependency inventory, and the verification guide describe the released packages
  instead of a release still to come.

## [0.1.1] - 2026-09-29

### Fixed

- Flatpak: the status icon now appears inside the sandbox. The tray asked for a D-Bus name a
  sandbox may not own and failed to start; it now registers with its unique name, and the Flatpak
  may talk to `org.kde.StatusNotifierWatcher`.

## [0.1.0] - 2026-09-27

The first release.

### Added

- Rule-language parser and renderer (`src/rules/`): all targets, the nine attributes, the six set
  operators, conditions, partial rules, queries, comments; byte-oriented, with `\xHH` escaping for
  non-UTF-8 device strings. Property tests and two cargo-fuzz targets (`fuzz/`).
- D-Bus layer (`src/dbus/`): typed proxies and client, safe rule removal by text identity, the
  coalescing event worker, supervision with reconnection, cancellable operations, the access probe
  sequence, and denial attribution. Tested against a mock bridge (`tests/support/`).
- Headless commands: `--diagnose`, `--version`, `--list-devices`, `--list-rules`.
- Device actions — allow, block, reject — with an explicit "this session only" / "permanently"
  choice, a confirmation for reject, and a Cancel button while waiting for authentication.
- Policy editing: a new-rule dialog with guided and text modes, live preview, and local
  validation; safe rule removal with a disambiguation dialog for identical rules; runtime
  parameters.
- GSettings schema, preferences, About dialog, desktop file, AppStream metainfo, and icons.
- Notifications for newly inserted, unauthorized devices, with an "Allow for this session" quick
  action that re-checks the device's identity before acting; background mode; a status icon
  (`ksni`) where the desktop supports one; "Start at login".
- Italian translation, and the gettext pipeline for more (`po/`, `make i18n`, `docs/translating.md`).
  The command-line output stays in English for bug reports.
- `cargo run --example demo-bridge`: a stand-in for USBGuard with invented devices, for screenshots
  and development; development builds reach it with `USBGUARD_GUI_BUS=session`.
- Packages: `.deb` and `.rpm` (`make package-deb`, `make package-rpm`), an Arch `PKGBUILD`, and a
  Flatpak manifest. No post-install scripts.
- Release pipeline: a `v*` tag builds the `.deb`, `.rpm`, Arch package, and Flatpak bundle, signs
  every asset with Sigstore, publishes the GitHub Release, and publishes the crate to crates.io
  (`docs/releasing.md`). Tags with a suffix are rehearsals: pre-releases, not published to crates.io.
- Read-only GTK 4 / libadwaita window: device list with filter, policy list in evaluation order,
  connection indicator, diagnostic dialog with per-distribution remedies.
- Phase 0 introspection results (`docs/dbus-introspection/`, `docs/architecture.md` §13.4) and an
  example Polkit rule, shipped inert (`packaging/70-usbguard-gui.rules.example`).
- Complete technical architecture specification (`docs/architecture.md`): the upstream D-Bus
  contract, the three-checkpoint access model, the process and concurrency design, the domain
  model, the rule-language grammar, the resilience test matrix, and the packaging plan.
- Project scaffold generated from
  [TemplateRepository](https://github.com/onyks-os/TemplateRepository) — governance, security
  policies, CI workflows, the Makefile contract, and the documentation site.
- Threat model and security assessment (`docs/security-assessment.md`), written before the code
  rather than after it.
- External interfaces reference (`docs/interfaces.md`): the command line, the GSettings schema, and
  the exit codes — the project's entire stable surface.

### Changed

- `docs/architecture.md`: only Fedora packages the D-Bus bridge separately; Debian, Ubuntu, and
  Arch ship it inside `usbguard` (checked in containers).
- `docs/architecture.md`: `listRules` takes a label filter, not a query — `""` lists every rule,
  `"match"` would have listed none. Found by introspection before any code depended on it.

[Unreleased]: https://github.com/onyks-os/USBGuardGUI/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/onyks-os/USBGuardGUI/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/onyks-os/USBGuardGUI/releases/tag/v0.1.0
