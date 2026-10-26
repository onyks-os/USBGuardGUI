# Development checkpoint — 2026-10-26

PR #61 and maintenance PRs #52–57 are merged. Continue preparing and publishing
0.2.0 through a verified release candidate on branch `release/0.2.0`. The user authorized signed-off commits,
PR management and release/tag publication; DCO consent already exists.

**Privacy:** never inspect the real host's desktop, applications, services or
physical devices. Use only the marked private VM with synthetic USB, or the
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

## Remaining release work

1. Version metadata, changelog and AppStream now identify 0.2.0; all four product
   milestones are assigned to this release. Local pre-integration Rust/lint and
   policy/i18n/coverage-tool tests, AppStream and strict MkDocs passed. The complete
   combination with the merged dependencies is being revalidated before release.
2. Push the release preparation PR; pass current-head CI and merge, then require
   main checks before tagging. No additional languages or AUR publication.
3. Follow `docs/releasing.md`: publish v0.2.0-rc.1, verify checksums/Sigstore
   identity, installed candidate package and pinned Flatpak validation; stable
   tag at the same verified commit. Existing native receipts from prior candidates
   must not be described as installed 0.2.0 validation.

Release-note draft: `target/release-020-notes.md`. Signature verifier is installed
only in `target/release-signature-venv/` (sigstore 4.5.1). The original CI coverage
report from final desktop commit is `target/ci-5f8874d-coverage/`; paths refer to
GitHub's checkout and were mapped in memory for local validation, not rewritten.

No v0.2.0 tag exists at this checkpoint. Stable remains 0.1.2.
