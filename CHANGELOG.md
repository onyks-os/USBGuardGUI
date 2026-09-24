<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing is released yet. See
[ROADMAP.md](https://github.com/onyks-os/USBGuardGUI/blob/main/ROADMAP.md) for the phase order.

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
- Packages: `.deb` and `.rpm` (`make package-deb`, `make package-rpm`), an Arch `PKGBUILD`, and a
  Flatpak manifest. No post-install scripts.
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

### Deprecated

### Removed

### Fixed

### Security

[Unreleased]: https://github.com/onyks-os/USBGuardGUI/commits/main
