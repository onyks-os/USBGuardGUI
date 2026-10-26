# scripts/

Operational scripts. Shell scripts are linted by `make lint-shell` (ShellCheck) and must:

- start with `#!/usr/bin/env bash` and `set -euo pipefail`;
- resolve its own directory rather than assuming the caller's working directory;
- be idempotent, or refuse to run twice with a clear message;
- print what it is about to change to the system *before* changing it.

| Script | Purpose |
| :----- | :------ |
| `verify.sh` | Full local gate; invoked by `make verify` and by hand. |
| `trace-filesystem.sh` | Trace background GUI startup and diagnostics on a private demo bus; see [validation notes](../docs/testing.md). |
| `verify-flatpak-runtime.py` | Install a bundle in private HOME/XDG/Flatpak directories, enforce the pinned GNOME runtime, and check CLI/schema/assets. |
| `reproducible-build.py` | Freeze build inputs, prepare the pinned builder, compare two offline GUI binaries, RPMs, DEBs, Arch packages and Flatpak bundles, and write the measured report. |
| `check-coverage.py` | Enforce >=98% production line coverage across the application, including GTK, and >90% per critical core file; reject missing, duplicate and malformed inputs. |
| `test-gui.sh` | Run GTK contracts on an isolated virtual display and private buses/configuration. |
| `verify-clean-install.py` | Manage a disposable rootless QEMU guest with synthetic USB, private disks/SSH credentials and no host passthrough. |
| `clean-install-guest-policy.sh` / `clean-install-guest-ui.py` | Prepare test policy and exercise real Polkit/PAM only inside the marked disposable guest. |
| `clean-install-guest-wayland.py` | Require the installed app's configured Wayland surface, committed buffer and frame callback in a guest Weston session. |
| `clean-install-guest-desktop.py` | Test native GNOME/Plasma rendering, accessibility, background/relaunch and tray in private guest sessions, with fatal criticals and repeated close/reopen cycles. |

There is deliberately no `install.sh` or `uninstall.sh`, and there will not be one. USBGuardGUI
touches no system configuration: installation is the distribution package's job, and the one action
a package performs — recompiling the GSettings schema — is a packaging convention, not a
configuration change. Production scripts must not edit `/etc/polkit-1/rules.d/` or the USBGuard IPC
access-control files; see
[`SECURITY.md`](../SECURITY.md) and [`docs/architecture.md`](../docs/architecture.md) §3.6.
The explicitly marked disposable-guest fixtures are the exception: they install
synthetic test policy inside that VM and refuse to run on an unmarked host.

`make reproducible` builds a pinned container and compares two offline GUI
binary, RPM, DEB, Arch and Flatpak builds in independent containers. It requires Python 3.11+ and rootless Podman
(or Docker via `CONTAINER_ENGINE=docker`). See [release verification](../docs/verification.md#5-reproducing-the-build)
for inputs, scope, and the explicit `--worktree` mode. `make test-reproducible`
runs its unit tests without a container or network.
