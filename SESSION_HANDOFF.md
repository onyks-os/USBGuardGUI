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
installation. Add its `usr/bin` to PATH and `usr/lib64` to LD_LIBRARY_PATH locally.
CI installs normal Ubuntu test packages. The Python suite now has 61 tests.
