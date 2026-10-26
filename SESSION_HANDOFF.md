# Active development checkpoint — resumed 2026-10-26

The user asked to continue. Branch `feat/desktop-quality`, PR #61 remains draft.
No new release or tag has been created. Privacy and DCO authorization below still
apply. All real desktop tests stay inside the marked synthetic Fedora VM.

Changes since the saved pause:

- Removed unsupported AT-SPI GrabFocus from diagnostic stress scripts. Formal
  driver uses editable text, ten hide/relaunch cycles, same-owner assertions,
  fatal GTK criticals and a final critical/error log gate. It records binary
  path/hash, GTK version, driver/helper/protocol hashes; `--binary` permits a
  diagnostic candidate without replacing the installed guest package.
- Controlled GNOME tests reproduce the GObject critical on BOTH original
  installed binary `71b56eff...fbbbd` and deferred-close experiment
  `b2c9367f...55a30`. The ineffective production experiment and its test were
  reverted. `tests/gui/window.rs` matches `41d0c2b` again; later dialog-helper call sites alter src/ui/window.rs separately.
- Exact guest package: `gtk4-4.22.5-2.fc44.x86_64`. GDB also catches the critical
  during repeated main-window close/relaunch, after preferences finished.
  New diagnostic target scripts log button type/parent/icon before callbacks.
  Do not infer an upstream cause yet; minimal GTK/X11 dialog and 100-cycle
  main-close examples pass, including a search and view-switcher version.
- Newer evidence: the GDB button breakpoint caught the FIRST activation on a
  `GtkButton` under `AdwSheetControls` (preferences close). GDB consumes the trap
  and may let later cases continue after detaching; their progress does NOT
  place the critical in the main-window close handler. A minimal GI example
  reproduces on isolated X11 when external dialog references are released;
  retaining it through `closed` until idle passes. The production candidate now
  centralizes this retention in ui::present_dialog for all transient dialogs.
  Fatal-critical GTK contracts pass and check native close plus eventual release.
  Final corrected-native GNOME run and fresh final coverage are in progress.
- VM currently RUNNING via `target/start-desktop-test-vm.py`; native test data
  remains in the same private VM. Diagnostic scripts and logs are ignored under
  `target/`. Exact ongoing tool session IDs are ephemeral, inspect named logs.
- `make coverage` passed inside Bubblewrap with isolated PID/proc/buses/display:
  5499/5606 = 98.0913%, but this measured the subsequently REVERTED experiment.
  Keep `target/coverage-experimental-close.json` separate. Re-measure final code.
  Force Mesa via `__EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/50_mesa.json`
  and `LIBGL_ALWAYS_SOFTWARE=1` in that container; otherwise Xvfb attempts the
  host NVIDIA library in its device-less namespace and aborts. Create private
  `/tmp/.X11-unix` mode 1777. No host device access or process inventory is needed.
- Driver tests: 20 passed; reproducibility checker tests: 30 passed; dependency
  policy negative tests: 6 passed (PATH must include target/tools/bin); i18n
  packaging tests: 7 passed. Cargo audit fetched current advisories and found
  no vulnerabilities; cargo-deny licenses/sources passed. Lint and pinned strict
  MkDocs build passed before latest docs-only updates; markdownlint unavailable.
- Updated scripts catalog and guest desktop reproduction procedure. Full final
  desktop acceptance, final receipts, fresh coverage and CI still required.
- GitHub PR #61 checked on resume: open/draft/mergeable, 17 published checks green
  for `41d0c2b`, not for local subsequent changes.

Useful new logs: `target/resume-{coverage,lint,audit,driver-tests,reproducible-tests,
dependency-policy-tests,i18n-tests,docs}.log`, `target/desktop-resumed-*.log`,
`target/gtk-close-{minimal,header}.log`. Guest originals saved as
`resumed-baseline-app.log` and `resumed-patched-app.log`; failed strict formal
runs do not leave passing receipts. Collect named files and stop VM when done.

---

# Development handoff — paused at user request, 2026-10-26

Resume the desktop-quality milestone, then prepare 0.2.0. The user explicitly
requested saving and stopping within two minutes. No 0.2.0 tag or publication
has started. DCO sign-offs and ordinary project PR/tag/release work remain
previously authorized; do not ask for that consent again.

**Privacy constraint:** never inspect the real host's desktop, application,
service or physical-device inventory. Desktop testing stays inside the marked,
headless Fedora VM with synthetic USB and private session buses/XDG directories.
Do not launch or inspect the user's actual desktop for these tests.

Current branch: `feat/desktop-quality`; PR #61 remains draft. Last pushed commit
is `41d0c2b`; CI run `40238388857` is entirely green, including application coverage
and reproducible packages. This pause creates a NEW LOCAL checkpoint commit;
its experimental changes are not yet published or CI validated.

Completed this session:

- Both GNOME Shell 50.6 and KWin/Plasma 6.7.5 passed six functional checks in the
  disposable guest: native Wayland frame, AT-SPI controls/filter, preferences,
  background notice/liveness, same-process relaunch, and GNOME no-watcher Quit
  or real Plasma tray registration/Activate/menu Quit.
- The baseline reports and raw app logs are under
  `target/clean-install-fedora-pr61/desktop-{gnome,kde}-{report.json,app.log}`.
  They match executable SHA256
  `71b56efffc9d97c6ad9228d8ae03356453c24a40f604d7e6977d6e27ca9fbbbd`
  (the feature candidate still identifies as 0.1.2). These new desktop reports
  are preliminary; do NOT treat them as clean final release receipts.
- New `scripts/clean-install-guest-desktop.py` records the repeatable test. It
  uses private dconf, a virtual output and software renderers; AT-SPI roots are
  restricted to the exact GUI PID. Native switch actions must target the inner
  actionable switch, not a similarly named label/wrapper. Both `button` and
  `push button` role names are accepted.
- `make test-clean-install-driver`: 17 tests passed, including the new guard
  proving host execution stops before GUI imports/process launch. Ruff passes.
- Cargo excludes `tests/test_clean_install*.py` alongside VM-only scripts, with
  a strengthened crate-content regression; these tests otherwise import helpers
  deliberately absent from published crates.
- README and user guides now explain details, attribute-only rule previews and
  private event history. `make docs-sync` updated the generated changelog page.
  Pinned MkDocs strict build passed using `target/release-docs-venv/bin/mkdocs
  build --strict --site-dir target/docs-desktop-quality-pinned`.
  Default `make docs-build` targets a sibling website checkout; use an explicit
  output INSIDE this repository. No outside write was made.

Open investigation before calling the milestone complete:

- GNOME emitted intermittent `GLib-GObject-CRITICAL: g_object_notify_by_pspec:
  assertion G_IS_OBJECT failed` despite successful functional cases. No matching
  critical was found in the collected KDE baseline log.
- GDB in the VM, attached only to the GUI PID with `G_DEBUG=fatal-criticals`,
  captured GTK `button_activate_timeout -> g_signal_emit -> g_log` on thread 1.
  See `docs/validation/desktop-quality-2026-10-26/investigation.md` and raw
  `target/desktop-gobject-backtrace.log`. The first debugger run passed cleanly;
  a later attempted stress run caught the critical.
- IMPORTANT newest finding: the draft stress loop calls AT-SPI
  `search.queryComponent().grabFocus()`, which raises an unsupported-operation
  error. GTK's inspected 4.22.5 AT-SPI component implementation explicitly rejects
  GrabFocus. Remove that unsupported call before interpreting stress failures;
  they may include cleanup after the automation aborts. Query the exact guest
  GTK package version before attributing the warning to a particular upstream
  version. No upstream defect or first-party cause is yet established.
- An EXPERIMENTAL, unproven source change currently defers background close to
  GLib idle and rechecks the latest preference. Its GUI regression is in
  `tests/gui/window.rs`. `make lint` passed, and a diagnostic release binary
  built. GUI contracts/coverage for this experiment have NOT been rerun. Keep
  it only if a controlled native before/after test demonstrates the fix; otherwise
  revert this experiment, preserving the unrelated tooling/docs changes.
- Pilot executable: `target/release/usbguard-gui`, copied only into guest home as
  `usbguard-gui-patched`; the installed original package was retained. Diagnostic
  scripts are `target/desktop-debug-test.py` and
  `target/desktop-debug-patched-test.py`. Both stress attempts stopped at the
  unsupported GrabFocus operation before completing any of the planned ten
  cycles. Do not describe those cycles as passed or the patch as validated.
- Formal desktop driver has not yet been run as its final typed source or had
  a strict critical-message gate added. Final receipts must identify the actual
  executed driver/helper/binary hashes. The earlier app coverage remains
  5491/5598 = 98.0886%; it is not a new measurement for the experimental code.

VM and artifacts:

`target/clean-install-fedora-pr61` has the reusable private disk, SSH metadata and
QMP socket. It was booted with 4 GiB/4 vCPUs TCG, no host passthrough/display.
Shutdown completed for this pause and the dedicated QMP endpoint was verified closed.
`target/start-desktop-test-vm.py` resumes that exact disk and refuses a concurrent
writer. New guest packages (GNOME, Plasma, Mesa, portal backends, GDB) were only
installed inside this VM; SELinux was not disabled. Credentials stay private
and uncommitted. Latest extra logs/scripts were archived successfully as
`target/clean-install-fedora-pr61/desktop-final-evidence.tar.gz` before shutdown.
A detailed working note is `target/desktop-quality-progress.md`; this handoff
supersedes its older experimental conclusions.

Next steps after the investigation: finalize desktop driver/receipts and accurate
ROADMAP/testing/verification status, update the stale scripts catalog, run required
checks with no real-host inventory access, commit/sign/push and finish PR #61 only
when green. Then prepare 0.2.0 metadata (Cargo.toml/lock, Arch pkgver, changelog and
its stale comparison links, AppStream input/generated metadata), CI and release
candidate `v0.2.0-rc.1` per docs/releasing.md. Verify packages, installed pinned
Flatpak and signatures before stable publication at the same verified commit.
No translation expansion or AUR work. Never move a published tag.

---

## Previous completed checkpoint (historical)

# Development handoff — 2026-10-25

Completed implementation follow-up: the user explicitly requested **98% coverage across the entire
application, including GTK**, plus an edge-case review. The local gate passes at **4841/4928 = 98.2346%**.
The implementation is on
`test/core-coverage-98` (the branch name predates the clarified whole-app scope).

- Production fixes: cancellable-operation events survive queue backpressure;
  Guided → Text releases its preview borrow before GTK's synchronous callbacks;
  parameter signals preserve structural denials and disable unknown values;
  policy error descriptions escape markup; one tray registration in flight
  follows the latest background preference, including rapid off/on changes.
- Unit tests moved to `tests/unit/`, graphical contracts to `tests/gui/`, preserving
  all existing tests. `make coverage-app` measures production source across GUI
  and headless feature configurations, with private display/buses/user dirs.
- Graphical tests cover real 20/180-second notices. Do not edit the launcher
  while it is executing. The 180-second test is intentionally a wall-clock wait.
- New coverage checker rejects omitted GUI files, malformed/duplicate reports,
  test-code inflation and percentages that only round to 98%.
- The original zbus migration / real clean-install work below is complete;
  these new GUI tests simulate the backend and do not replace its PAM receipts.


The requested zbus/ksni migration and clean-install verification are implemented
and locally verified. No translation expansion or AUR publication was undertaken.

- zbus 6.1 and Rust 1.89 replace the old dependency/compiler baseline. The fixed
  SNI/menu implementation runs directly on zbus, replacing ksni and the deprecated
  zvariant stack. Architecture, dependency inventory and ADR 0002 describe it.
- All four real GUI/PAM cases pass on Ubuntu 24.04.5, Debian 13.7, Fedora 44 and
  Arch rolling: no agent, cancelled authentication, correct-password runtime block
  with unchanged permanent rules, daemon restart/GUI reconnection.
- Tests also verify wire properties/actions, both tooltip signals, watcher restart,
  mutation error classification and installed artifact identity. Core per-file
  coverage stays above 90%; the dependency audit has no findings or notices.
- [Testing](docs/testing.md#zbus-6-and-real-clean-system-validation-2026-10-2425)
  and [verification](docs/verification.md#post-012-zbus-6-validation-2026-10-2425)
  contain exact scope, reproduction and committed receipts.

Checkpoint artifacts under ignored `target/zbus6-repro-final` are measured
modified-worktree builds retaining version 0.1.2, distinct from the published
0.1.2 release. The native binary hash in all guest receipts is
`e6f620ab6756d34849a15357b9967fe1b07541a0b509686f3d930484ff53cf82`.
The CI reproduction gate measures the final PR source independently.

Local test VM disks and evidence remain under `target/clean-install-*`, with
credentials private and never committed. Stop VMs after collecting evidence.
Use generic Debian images with USB support, start the packaged modern Polkit
helper socket when needed, and reload fixture policy after writing it.
Do not enable SUID or disable SELinux to make a test pass.

No new release/tag is part of this implementation handoff. Publication needs the
usual versioning and release gates. Signed-off-by with the configured Git identity
was explicitly approved for this work; avoid asking for that consent again.

Developer tools: `/home/onyks/.cargo/bin` and `target/tools/bin` (cargo-deny 0.20.2,
cargo-llvm-cov 0.9.1). Use `/bin/bash`, `login:false`. The host lacks /dev/kvm,
so VM execution uses TCG. Historical detailed pause instructions are preserved
in commit `ccfb96d`; the work described there has now been completed.

Whole-app evidence: `docs/validation/application-coverage-2026-10-25.json`;
full LLVM JSON `target/coverage-application.json`; HTML
`target/llvm-cov/html/index.html`; logs `target/coverage-98-complete.log`,
`target/coverage-98-verify-final.log`, `target/coverage-98-msrv.log`,
`target/coverage-98-properties.log`. The native test host lacked Xvfb/xdotool:
RPMs were downloaded/extracted under `target/coverage-tools/root` without a host
installation. `make coverage` now discovers both local tool locations automatically, including
the unpacked desktop libraries; no manual PATH/LD_LIBRARY_PATH exports are needed.
Missing dependencies fail before tests with install instructions. Four new tool
discovery/preflight regressions bring the Python suite to 65 tests. The diagnostic
GUI regression now accepts the no-agent CI case and separately checks an IPC
denial with copyable commands. CI installs normal Ubuntu test packages.

Coverage command follow-up: `make coverage` succeeded without any manually added
PATH or LD_LIBRARY_PATH (`target/coverage-user-entrypoint.log`), still 98.2346%.
`target/coverage-tools-gui.log` verifies the agent-independent diagnostic fixture;
`target/coverage-tools-tests.log` has 13 checker/tool tests and Clippy is clean.
