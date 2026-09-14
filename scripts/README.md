# scripts/

Operational shell scripts. Everything here is linted by `make lint-shell` (ShellCheck) and must:

- start with `#!/usr/bin/env bash` and `set -euo pipefail`;
- resolve its own directory rather than assuming the caller's working directory;
- be idempotent, or refuse to run twice with a clear message;
- print what it is about to change to the system *before* changing it.

| Script | Purpose |
| :----- | :------ |
| `verify.sh` | Full local gate; invoked by `make verify` in CI and by hand. |

There is deliberately no `install.sh` or `uninstall.sh`, and there will not be one. USBGuardGUI
touches no system configuration: installation is the distribution package's job, and the one action
a package performs — recompiling the GSettings schema — is a packaging convention, not a
configuration change. A script here that edited `/etc/polkit-1/rules.d/` or the USBGuard IPC
access-control files would contradict the program's central property; see
[`SECURITY.md`](../SECURITY.md) and [`docs/architecture.md`](../docs/architecture.md) §3.6.
