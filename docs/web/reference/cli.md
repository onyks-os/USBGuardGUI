# Reference: Command Line Interface

Exhaustive, non-narrative reference for every command and option. This page mirrors
[`docs/interfaces.md`](https://github.com/onyks-os/USBGuardGUI/blob/main/docs/interfaces.md);
both must be updated together.

---

## Commands

The program is a single binary with no subcommands.

### `usbguard-gui`

Present the main window. Requires no privileges, and must not be run as root.

```text
Usage: usbguard-gui [OPTIONS]
```

| Option | Type | Default | Description |
| :----- | :--- | :------ | :---------- |
| `--background` | flag | off | Start without presenting the window. Overrides the `start-in-background` setting. |
| `--diagnose` | flag | — | Run the access probe sequence, print the result, and exit without opening a window. |
| `--version`, `-V` | flag | — | Print the version, plus the USBGuard D-Bus API level detected on the bus, and exit. |
| `--list-devices` | flag | — | Print the devices the daemon knows, sorted by port, and exit. |
| `--list-rules` | flag | — | Print the ruleset in evaluation order, and exit. |
| `--help`, `-h` | flag | — | Show usage and exit. |

A second launch does not start a second process: `adw::Application` raises the running window
instead.

Example:

```bash
usbguard-gui --background
```

### `usbguard-gui --diagnose`

Run the probe sequence and report the access state. Exists so that a bug report can carry one
command's output instead of a description of a dialog, and so the checks are exercisable in CI
without a display.

```text
Usage: usbguard-gui --diagnose
```

The probes run in order and stop at the first failure:

| # | Question | Outcome if it fails |
| :- | :------- | :------------------ |
| 1 | Is there a system bus? | `BusUnavailable` |
| 2 | Does anything own `org.usbguard1`, or is the name at least activatable? | `BridgeNotInstalled` / `BridgeNotRunning` |
| 3 | Is a Polkit agent present in this session? | Recorded, not fatal — used to explain a later silent denial |
| 4 | Can a parameter be read? | Falls through to classification |
| 5 | Which checkpoint denied it? | `DeniedByBusPolicy`, `DeniedByPolkit`, `DeniedByIpcAcl`, `NoPolkitAgent`, or `DeniedUnattributed` |

Write access is deliberately **not** probed: the only honest test of write access is a write, and a
test write would change the system's USB policy in order to find out whether it is allowed to. The
first real operation carries that test, and reports its failure in the context where the user has
the intent to make sense of it.

Example:

```bash
usbguard-gui --diagnose || echo "not usable yet, exit code $?"
```

---

## Environment

| Variable | Default | Description |
| :------- | :------ | :---------- |
| `USBGUARD_GUI_LOG` | `warn` | `tracing` `EnvFilter` directive. The `debug` and `trace` levels record device names, serial numbers, and hashes; `info` and above never do. |

---

## Exit Codes

| Code | Meaning |
| :--- | :------ |
| `0`  | Success. For `--diagnose`, the access state is `Ok`. |
| `1`  | Generic error. |
| `2`  | Invalid usage. |
| `3`  | `--diagnose` only: reachable, but access denied at one of the three checkpoints. |
| `4`  | `--diagnose` only: the D-Bus bridge is not installed, or is not running. |
| `5`  | `--diagnose` only: no system bus is available in this session. |
