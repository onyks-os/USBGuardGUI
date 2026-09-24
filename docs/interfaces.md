<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# USBGuardGUI — External Interfaces Reference

This document is the contract between USBGuardGUI and everything outside it. Any change to what
is described here is a change to the public interface and must follow
[Semantic Versioning](https://semver.org/).

> **Status:** the command line, exit codes, and GSettings schema are implemented. Nothing ships in
> a release yet.

## Table of Contents

1. [Command Line Interface](#1-command-line-interface)
2. [Configuration](#2-configuration)
3. [Exit Codes](#3-exit-codes)
4. [Programmatic API](#4-programmatic-api)
5. [Files & System Integration](#5-files--system-integration)
6. [External Network Endpoints](#6-external-network-endpoints)

---

## 1. Command Line Interface

The program is a single binary, `usbguard-gui`, with no subcommands. Everything it does is either
the window or one of the flags below.

| Command | Description | Requires privileges |
| :------ | :---------- | :------------------ |
| `usbguard-gui` | Present the main window. | No — and it must not be run as root. |
| `usbguard-gui --background` | Start without presenting the window, as a tray item or a background service. | No |
| `usbguard-gui --diagnose` | Run the access probe sequence, print the result to stdout, and exit. | No |
| `usbguard-gui --list-devices` | Print the devices the daemon knows, sorted by port, and exit. | No |
| `usbguard-gui --list-rules` | Print the ruleset in evaluation order, and exit. | No |

`--diagnose` exists so that a user reporting a problem can paste one command's output instead of
describing a dialog, and so the checks are exercisable in CI without a display.

Single-instance behaviour comes from `adw::Application` with `ApplicationFlags::HANDLES_COMMAND_LINE`:
a second launch raises the running window rather than starting a second process.

### Global Options

| Option | Type | Default | Description |
| :----- | :--- | :------ | :---------- |
| `--help` | flag | — | Show usage and exit. |
| `--version` | flag | — | Print the version, plus the USBGuard D-Bus API level detected on the bus, and exit. The bridge exposes no version number; the API level is inferred from its interface (`architecture.md` §13.4). |
| `--background` | flag | off | Start without presenting the window. Overrides the `start-in-background` setting. |
| `--diagnose` | flag | — | Run the probe sequence, print the result, and exit. |
| `--list-devices` | flag | — | Print the device table and exit. Prints device names and serial numbers to stdout — the user asked for them; they are never logged. |
| `--list-rules` | flag | — | Print the ruleset and exit. |

---

## 2. Configuration

Settings are persisted through GSettings under the schema ID `io.github.onyks_os.UsbguardGui`.

**Nothing in this table is security policy.** No setting of this program has any effect on the
USBGuard daemon, on the ruleset, or on what the user is authorized to do. These control presentation
and startup behaviour only.

| Key | Type | Default | Description |
| :-- | :--- | :------ | :---------- |
| `start-in-background` | boolean | `false` | Start without presenting the window. |
| `notify-inserted` | boolean | `true` | Send a desktop notification when an unauthorized device is inserted. |
| `run-in-background` | boolean | `false` | Keep running after the window is closed, so insertions are still announced. Shows the tray icon where the desktop has a StatusNotifierWatcher. |
| `default-persistence` | enum | `runtime-only` | Which option is preselected in the device action popover. Defaults to the non-persisting one deliberately: for a security tool, the less destructive default is the one whose effect disappears on restart. |
| `show-blocked-only` | boolean | `false` | Device list filter. |
| `tray-notice-shown` | boolean | `false` | Whether the one-time "still running in the background" notice has been given. |
| `window-width` | integer | — | Window geometry. |
| `window-height` | integer | — | Window geometry. |
| `window-maximized` | boolean | `false` | Window geometry. |

### Environment variables

| Variable | Type | Default | Description |
| :------- | :--- | :------ | :---------- |
| `USBGUARD_GUI_LOG` | `tracing` `EnvFilter` directive | `warn` | Log verbosity and per-target filtering. See [Logging](#logging). |

### Logging

`tracing` with an `EnvFilter`. Spans wrap each D-Bus call and each event-coalescing window, which is
also how the latency figures in [`architecture.md`](architecture.md) §13.2 are measured.

**Device names, serial numbers, and hashes are never logged above `debug`.** The `debug` level
documents, in its own output, that it records device identifiers. A default-level log attached to a
bug report must not be an inventory of the hardware the reporter owns.

Precedence: command-line flags override environment variables, which override GSettings, which
overrides built-in defaults.

---

## 3. Exit Codes

| Code | Meaning |
| :--- | :------ |
| `0`  | Success. For `--diagnose`, this additionally means the access state is `Ok`. |
| `1`  | Generic error. |
| `2`  | Invalid usage or arguments. |
| `3`  | `--diagnose` only: the daemon is reachable but access is denied at one of the three checkpoints. |
| `4`  | `--diagnose` only: the D-Bus bridge is not installed, or is installed but not running. |
| `5`  | `--diagnose` only: no system bus is available in this session. |

The distinct `--diagnose` codes exist so that a script or a CI job can branch on *which* precondition
is missing without parsing the human-readable output.

---

## 4. Programmatic API

**None.** USBGuardGUI ships a binary, not a library. The `usbguard_gui` crate is an implementation
detail of that binary: nothing in it is a supported entry point, and anything in it may change in
any release without a major version bump.

If you want programmatic access to USBGuard, use the daemon's own D-Bus interface directly, or the
`usbguard` CLI. This program has no privileges you do not already have.

The stable surface of this project is exactly three things: the command line above, the GSettings
schema above, and the exit codes above.

---

## 5. Files & System Integration

### Paths the program reads or writes

| Path | Purpose | Lifetime |
| :--- | :------ | :------- |
| `~/.config/dconf/user` | GSettings backend store, written indirectly through the GSettings API. Holds only the keys in §2. | Persistent |
| `~/.config/autostart/io.github.onyks_os.UsbguardGui.desktop` | Written only when the user turns on "Start at login" in the preferences, removed when they turn it off. Starts the program with `--background`. Not offered under Flatpak. | Until turned off |
| `/usr/lib/os-release`, `/run/host/os-release` | Read to choose the per-distribution install command in a remedy. | Read only |
| `/proc/*/comm` | Read by `--diagnose` to recognize a running Polkit agent (heuristic; skipped under Flatpak). | Read only |
| `$XDG_RUNTIME_DIR/bus`, `/run/dbus/system_bus_socket` | Session and system D-Bus sockets. | Process lifetime |

That is the complete list. In particular:

> **The program reads and writes nothing under `/etc`, `/var`, or `/sys`.** This is an invariant,
> not a convention: it is verified by `strace` filtered on those three prefixes, which must report
> zero accesses. `usbguard-daemon.conf`, the IPC access-control files, and `rules.conf` are all
> root-owned and are never touched — the program diagnoses them through the daemon and prints the
> command an administrator would run.

### Paths installed by the package

| Path | Purpose | Lifetime |
| :--- | :------ | :------- |
| `/usr/bin/usbguard-gui` | The binary. | Package lifetime |
| `/usr/share/applications/io.github.onyks_os.UsbguardGui.desktop` | Desktop entry. | Package lifetime |
| `/usr/share/metainfo/io.github.onyks_os.UsbguardGui.metainfo.xml` | AppStream metadata. | Package lifetime |
| `/usr/share/glib-2.0/schemas/io.github.onyks_os.UsbguardGui.gschema.xml` | GSettings schema. | Package lifetime |
| `/usr/share/icons/hicolor/…/io.github.onyks_os.UsbguardGui.svg` | Application icon. | Package lifetime |
| `/usr/share/locale/…/usbguard-gui.mo` | Translation catalogues. | Package lifetime |
| `/usr/share/doc/usbguard-gui/70-usbguard-gui.rules.example` | Example Polkit rule — **shipped inert, never installed into `/etc/polkit-1/rules.d/`.** | Package lifetime |

The post-install script modifies no system configuration. Its only action is the GSettings schema
recompilation that packaging conventions require.

### D-Bus interfaces consumed

| Bus | Name | Purpose | Required |
| :-- | :--- | :------ | :------- |
| System | `org.usbguard1` | The USBGuard D-Bus bridge — devices, rules, parameters, and signals. | Yes. Its absence is a distinct diagnostic state, not a generic failure. |
| System | `org.freedesktop.DBus` | `NameHasOwner` and `ListActivatableNames`, used by the probe sequence to tell "package not installed" from "service not running" without touching the filesystem. | Yes |
| Session | `org.freedesktop.PolicyKit1.AuthenticationAgent` | Presence check only — never called. Used to explain a silent denial. | No |
| Session | `org.freedesktop.Notifications` | Desktop notifications, sent through GIO (which uses the notification portal under Flatpak); `GetCapabilities` decides whether quick-action buttons are offered. | No — degrades to no notifications. |
| Session | `org.kde.StatusNotifierWatcher` | Tray icon. | No — degrades to background mode, visibly and with a one-time notice. |

The program **exports** one D-Bus name, its own application ID `io.github.onyks_os.UsbguardGui` on
the session bus, which is what `GApplication` uses for single-instance behaviour. It exports no
interface anyone else is expected to call.

---

## 6. External Network Endpoints

**None.** The program contacts no host, opens no socket beyond the local D-Bus sockets listed in §5,
and performs no DNS resolution.

| Endpoint | Purpose | When contacted | Opt-out |
| :------- | :------ | :------------- | :------ |
| — | — | Never | Not applicable |

There is no telemetry, no crash reporting, no update check, and no analytics — not as an opt-out, but
as an absence: there is no code that could perform one. A local firewall rule denying the process all
network access changes nothing about its behaviour, which is the test to apply if you would rather
verify this than trust it.
