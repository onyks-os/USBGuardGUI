# Release Notes & Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing is released yet. The repository holds the specification and the project infrastructure;
the program itself has not been written. See
[ROADMAP.md](https://github.com/onyks-os/USBGuardGUI/blob/main/ROADMAP.md) for the phase order.

### Added

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

### Deprecated

### Removed

### Fixed

### Security

[Unreleased]: https://github.com/onyks-os/USBGuardGUI/commits/main
