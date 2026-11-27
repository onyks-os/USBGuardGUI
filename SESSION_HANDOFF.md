# Development checkpoint — 2026-10-26

**0.2.0 is published and installed on the user's Fedora host.** The approved
feature roadmap is complete; continue in maintenance mode, not release preparation.
The user authorized signed-off commits, PR management and release/tag publication;
DCO consent already exists. No additional languages or AUR work is requested.

**Privacy:** never inspect the real host's desktop, applications, services or
physical devices. The explicitly requested USBGuardGUI package upgrade and narrow
package/API verification were authorized; this does not authorize unrelated host
inspection. For graphical tests use only the marked private VM with synthetic USB, or the
narrow namespace wrapper `target/run-private-tests.py`. No subagents requested.

## Completed and verified

- Production fix at pushed `ddd8977`: retain transient dialogs through native
  close callbacks, then release at GLib idle. Preferences/About contract verifies
  native close and eventual release. Earlier main-window deferral was ineffective
  and reverted. Investigation: `docs/validation/desktop-quality-2026-10-26/`.
- Final whole-application coverage: **5500/5607 = 98.0917%**, GUI included.
  Receipt identifies source and Rust-test digests. Log:
  `target/dialog-lifetime-coverage.log`; HTML `target/llvm-cov/html/index.html`.
- Final native desktop tests passed: GNOME seven cases/10 cycles, Plasma seven
  cases/30 cycles, fatal criticals, no debugger, same owner, single notice and
  clean exit. Plasma uses its real watcher/tray. Both receipts identify diagnostic
  executable SHA256 `d7d4337c009f06e3799f865a4c74e0f8b8b8bb4e6a51591741a405d2b9ad093a`,
  NOT an installed release package. Version at build remains 0.1.2.
- The final driver uses atomic AT-SPI root GetChildren during window hiding.
  GDB traced the separate stale-index critical to GTK 4.22.5's root handler.
  This corrects our observer; it does not fix GTK for other accessibility clients.
- All 17 PR checks passed on ddd8977, including coverage, reproducibility of every
  package format, stable/beta/MSRV, lint, audit, DCO and CodeQL. Final observer/docs commit `5f8874d` also passed all 17 checks before
  merge. Main integration commit is `2b64f6e`. Maintenance updates include
  Tokio 1.54.1, proptest 1.12.0 and pinned CI actions; each PR passed all 17 checks.
- Local checks already passed: driver 20 tests, reproducibility 30, dependency
  policy 6, i18n 7; audit/deny, lint, pinned strict MkDocs. Markdownlint unavailable.

## VM and local evidence

The private Fedora VM is **STOPPED** (graceful shutdown and private QMP socket
checked). Its directory is `target/clean-install-fedora-pr61`; keys/credentials
must never be printed or committed. Evidence archive:
`target/clean-install-fedora-pr61/desktop-accepted-evidence.tar.gz`.
Final raw reports/logs: `target/desktop-{gnome,kde}-atomic-{report.json,app.log}`.
Resume only if needed via `target/start-desktop-test-vm.py` (refuses second writer).

Host GUI/coverage tests MUST use `target/run-private-tests.py`: empty mount root,
private proc/dev/tmp/run/sys/buses/network, only explicit code/toolchain binds,
software Mesa. Do not use the older whole-root bind or the real desktop.

## Release completed; maintenance next

- PR #62 and all preceding release/maintenance PRs are merged. Stable `v0.2.0`
  and candidate `v0.2.0-rc.1` target `8add6f16c4acd4d1c9698708a31d2b2c2af37986`.
- Integrated CI `40373074672`, candidate release `40378484865` and stable release
  `40383201161` succeeded. GitHub release and crates.io 0.2.0 are live.
- All 13 artifact checksums and 14 stable Sigstore signatures were verified.
  Receipt: `target/release-0.2.0-published/local-verification.json`.
- Exact candidate/stable RPM passed 5/5 installed Fedora acceptance cases.
  Use `target/release-020-fedora-package-validation-final.json`; the earlier
  non-final receipt tested an old installed binary and is not valid acceptance
  evidence. Use `dnf install`, not `dnf reinstall`, to upgrade across versions.
- The user's host was upgraded from 0.1.2 to 0.2.0 using that verified RPM.
  Outside the sandbox, `rpm -V usbguard-gui` passed and `usbguard-gui --version`
  reported 0.2.0 and a reachable USBGuard API. Installed binary SHA256:
  `852f8232baa90fcb675bfd08cffccca7ef740eb0f5cf8dc1f5263553afefe5ff`.
- Merged branches were removed; only main remained. GitHub automatic deletion
  of merged branches is enabled. The private VM remains stopped.
- Current task: align documentation with the published release and prepare a
  Reddit announcement in `docs/announcements/reddit-0.2.0.md`. The announcement
  is a draft for the user to post; it has not been submitted to Reddit.

Maintenance covers bugs, security/dependency review and desktop/distribution
compatibility. There is no pending approved feature milestone. Do not restart
release preparation or repeat the full test suite for documentation-only edits.
Release evidence and its scope are summarized in `docs/verification.md`.
