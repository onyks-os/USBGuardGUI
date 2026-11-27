# Development checkpoint — 2026-11-27

**0.2.1 is published** (GitHub Release *Latest* and crates.io). The approved
feature roadmap is complete; continue in maintenance mode, not release preparation.
The user authorized signed-off commits, PR management and release/tag publication;
DCO consent already exists. No new features are proposed. This session did not touch
the user's installed package (0.2.0 at the previous checkpoint).

**Privacy:** never inspect the real host's desktop, applications, services or
physical devices. The explicitly requested USBGuardGUI package upgrade and narrow
package/API verification were authorized; this does not authorize unrelated host
inspection. For graphical tests use only the marked private VM with synthetic USB, or the
narrow namespace wrapper `target/run-private-tests.py`. No subagents requested.

## 0.2.1 release (2026-11-27)

- Merged: #71 (pt_BR, contributor Rodrigo de Oliveira), #72 (coverage gate 95%,
  CI jobs on `ubuntu-24.04`), Dependabot #65–#70, #73 (0.2.1 preparation), #74
  (Flatpak runtime pin). Candidate `v0.2.1-rc.2` and stable `v0.2.1` target
  `9cbfd2153f3e8c07cc4bedca3bbfdff5d8701386`.
- `v0.2.1-rc.1` published nothing: Flathub had pruned the pinned GNOME 50 runtime
  commit (404). Flathub keeps ~5 weeks; `docs/releasing.md` step 5 now says to
  refresh `packaging/reproducible/flatpak-runtime.json` before every tag.
- Candidate and stable: 13 checksums and 14 Sigstore signatures verified, all six
  artifacts byte-identical, crates.io checksum equals the signed crate. Receipt:
  `target/release-0.2.1-published/local-verification.json`. No Fedora-guest
  package acceptance run was repeated for 0.2.1 (see `docs/verification.md`).
- Coverage on #73: 5294/5428 = 97.5313% (gate >=95%).
- Known external issue: USBGuard #636 (runtime rules persisted by a later permanent
  change). The UI and docs state it; nothing to fix on our side unless upstream changes.
- `docs/announcements/reddit-0.2.0.md` in the user's working copy is an untracked
  draft, never committed or posted.

## Earlier 0.2.0 evidence

- Whole-application coverage at 0.2.0: **5500/5607 = 98.0917%**, GUI included.
- Native GNOME/Plasma desktop tests and the 5/5 installed Fedora acceptance run
  are described in `docs/verification.md`; their receipts remain under `target/`.

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

Maintenance covers bugs, security/dependency review and desktop/distribution
compatibility. There is no pending approved feature milestone. Do not restart
release preparation or repeat the full test suite for documentation-only edits.
Release evidence and its scope are summarized in `docs/verification.md`.
