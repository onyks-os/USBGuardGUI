# Paused development checkpoint — 2026-10-24

The user explicitly requested saving state and ending work. Do not resume automatically.

## Requested scope and authorizations

Complete the zbus/ksni migration and clean-install verification. No new language work,
AUR publication or unrelated future milestones. The user wants minimal approval prompts.
Signed-off-by using the configured Git identity was explicitly approved for this work:
`onyks-os <129986281+onyks-os@users.noreply.github.com>`.

Branch: `feat/zbus6-clean-install-validation`, based on main `3472a156`.
This is an unfinished checkpoint, not a release. No tag or PR has been published for it.

## Implemented and verified

- zbus 6.1 replaces the zbus 5 tree; ksni is removed. The fixed SNI/dbusmenu
  implementation uses zbus directly, as architecture §9.6 permits. Native and
  Flatpak registration, menu actions, tooltip signals, watcher restart and
  shutdown have private-bus tests. Typed zbus errors retain denial classification.
- Rust minimum/CI matrix and pinned reproducible builder are 1.89 / 1.89.0.
  The official Rust archive checksum is committed with the recipe.
- Rust: 114 tests pass (`target/zbus6-tests-final.log`); the separate GTK
  authentication-cancellation regression passes (`target/zbus6-gtk.log`).
  All-target/all-feature Rust 1.89 compilation passes (`target/zbus6-msrv.log`).
  Stable Clippy and rustfmt pass (`target/zbus6-clippy-final.log`).
- Core per-file coverage passes: builder 98.40%, lexer 96.58%, rules/mod 97.97%,
  parser 93.06%, renderer 97.80%, commands 90.91%, errors 93.02%.
  Report: `target/zbus6-coverage.json`.
- Audit: zero vulnerabilities and zero informational warnings
  (`target/zbus6-audit.json`). License/source gate passes.
- Python: 49 tests pass, including six VM-driver isolation/input guard tests
  (`target/zbus6-python-tests-final.log`). Ruff and ShellCheck pass.
- Both offline builds produce identical native binary, RPM, DEB, Arch package
  and Flatpak bundle. `target/zbus6-repro-final/report.json` identifies the
  modified-worktree snapshot and recipe. The native binary SHA-256 is
  `e6f620ab6756d34849a15357b9967fe1b07541a0b509686f3d930484ff53cf82`.
  Source snapshot: `074b746be442aa7c1a9cc89911540f4d330ceb5bbe35e9d3213a60d19dacfdcb`.
  Packages are checkpoint builds retaining version 0.1.2, NOT the published 0.1.2 artifacts.
- Installed Flatpak CLI/schema/assets checks pass against the pinned GNOME 50
  runtime (`target/zbus6-flatpak-runtime-v2/report.json`). The verifier now uses
  `--no-documents-portal` for these headless checks, avoiding an unrelated host
  document-portal mount failure.
- Ubuntu 24.04.5 with USBGuard 1.1.2 passes all real GUI/PAM checks: no agent,
  cancelled authentication, successful runtime block with unchanged persistent
  rules, daemon restart/reconnection and restoration of persistent allow.
  Evidence: `target/clean-install-ubuntu-v2/report.json` and `guest-evidence.tar.gz`.

## Clean VMs: incomplete work

Scripts: `scripts/verify-clean-install.py`, `clean-install-guest-setup.sh`,
`clean-install-guest-policy.sh`, `clean-install-guest-ui.py`.

These are guarded, disposable VM tooling, not product installers/helpers.
They use hash-verified official images, rootless QEMU/TCG, loopback-only SSH,
private overlay disks and synthetic USB serial `usbguard-clean-test`.
They expose no host filesystem or physical USB device. The explicit test-only
Polkit rule grants reads and requires real PAM AUTH_SELF for every write;
distribution defaults are recorded separately. Credentials are private under
`target/clean-install-*/credentials.json`: never print, commit or publish them.

Ubuntu is complete. Fedora 44, Arch and Debian 13 native packages install and
start their CLI, but their full GUI/PAM suites were still being worked on at pause:

- Fedora uses `mate-polkit` because `polkit-gnome` is unavailable in its official
  repositories. SELinux remains enabled. Start the AT-SPI launcher and registry
  explicitly; automatic dbus-daemon execution was denied in the synthetic session.
  Latest observed progress: no-agent case passed. The MATE dialog Cancel lookup
  differed from GNOME; the latest script uses Escape for cancellation and Enter
  for authentication. This latest path still needs a completed verification.
- Arch uses Polkit 128 and a socket-activated helper, deliberately without SUID.
  Installing it after sockets.target was reached did not start its new socket.
  `systemctl start polkit-agent-helper.socket` was run inside the VM; the guest
  setup now does this when the unit exists. Do not add SUID permissions.
  Latest observed progress: no-agent case passed; complete the other cases.
  The test stopped before a completed cancellation/password verification.
- Debian's genericcloud kernel has no xHCI driver. Installed the official
  `linux-image-amd64` package, then used grub-reboot to select the non-cloud
  `6.12.113+deb13-amd64` kernel. Its USB device is now visible. Setup was being
  rerun to generate the fixture policy and enable both USBGuard services.
  Latest validation reached the GUI but failed the initial `target() == 'allow'`
  assertion: reload/restart the daemon after writing the generated fixture policy
  before testing. `enable --now` does not reload an already-running daemon.
  Prefer the official **generic** image, not genericcloud, for future fresh runs.
- AT-SPI role/action differences required adapting the driver: editable role
  `entry`, nested menu-button actions, runtime-choice Enter, real-agent Escape,
  a private XDG_RUNTIME_DIR/display, and explicit AT-SPI services.
  Latest driver changes need end-to-end confirmation on all remaining systems.
  Private document-portal FUSE mounts caused cleanup PermissionError, sometimes
  masking an earlier failure. `ignore_cleanup_errors=True` was added but latest
  traces still contain cleanup errors; unmount the test-owned portal or otherwise
  handle this explicitly. VM disposal removes these guest-only mounts. Test logs must not be mistaken for product defects.

VM directories: `target/clean-install-ubuntu-v2`, `target/clean-install-fedora`,
`target/clean-install-arch`, `target/clean-install-debian`.
All four VMs were synced, evidence was archived and they were stopped successfully.
Stop/checkpoint result: `target/session-checkpoint-vms.json`.
For VMs that were running at pause, `qemu-command.json` and `resume.sh` preserve
exact launch arguments. Only resume after checking that the recorded VM is
stopped and its loopback port is free. Their overlay disks and SSH credentials
remain intact. Do not call `prepare` on these existing directories or redownload images.

Typical continuation after resuming a VM:

```sh
python3 scripts/verify-clean-install.py ssh --output target/clean-install-fedora true
python3 scripts/verify-clean-install.py validate --output target/clean-install-fedora \
  --package target/zbus6-repro-final/first/usbguard-gui-0.1.2-1.x86_64.rpm
python3 scripts/verify-clean-install.py collect --output target/clean-install-fedora
python3 scripts/verify-clean-install.py stop --output target/clean-install-fedora
```

After successful real tests, record receipts and exact scope in docs/testing.md
and docs/verification.md. Recheck the latest scripts and documentation; run make
verify with the locally installed tools on PATH. Open/review the PR, wait for all
checks before merging. No release/tag until all required gates are complete.
A draft PR description is saved at `/tmp/usbguard-zbus6-pr.md` (temporary, optional).

## Environment notes

Use `/bin/bash`, `login:false`; Cargo is at `/home/onyks/.cargo/bin`.
`cargo-deny` 0.20.2 and `cargo-llvm-cov` 0.9.1 are installed in `target/tools/bin`.
No /dev/kvm is available; software virtualization makes these tests slow.

The host DNS stub intermittently fails for Rust/crates and Debian mirrors.
No host DNS configuration was changed. Temporary local workarounds:
`/tmp/usbguard-cargo-network.py` tunnels TLS for index/static.crates.io using
verified DNS-over-HTTPS answers; `/tmp/usbguard-network-bin/podman` sets DNS
only for networked containers and preserves `--network=none` builds.
Reproducible builder image is cached at
`localhost/usbguard-gui-reproducible:ef5b483d1274d70f`.
Temporary helpers may disappear; recreate only if needed. Existing compiled
artifacts, images and logs are under ignored `target/` and are not Git-backed.
