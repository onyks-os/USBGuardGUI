# Tutorial: Quickstart

!!! warning "This tutorial does not work yet"
    USBGuardGUI has not been implemented. The steps below describe what the first release will do,
    and exist so the design can be reviewed against a concrete user experience. Step 1 is the only
    one you can run today, and only in the sense that the repository will clone and the scaffold
    will build.

This tutorial takes you from nothing installed to a working USBGuardGUI setup. It is a learning
exercise: follow it in order, and do not substitute your own values until the end.

**You will need:** a Linux desktop session, the USBGuard daemon installed, Rust 1.85 or newer, and
about 10 minutes.

---

## 1. Install

From source, which is the only route until a release exists:

```bash
git clone https://github.com/onyks-os/USBGuardGUI.git
cd USBGuardGUI
make setup
make build
```

On Fedora, `make setup` expects these system packages:

```bash
sudo dnf install gtk4-devel libadwaita-devel glib2-devel pkgconf-pkg-config gcc
```

Verify the installation:

```bash
usbguard-gui --version
```

You should see the version number printed, followed by the USBGuard version detected on the system
bus. If the second half is missing, the daemon or its bridge is not reachable — which is exactly
what step 2 is for.

## 2. Find out whether you are allowed to talk to the daemon

```bash
usbguard-gui --diagnose
```

**What just happened:** the program ran its probe sequence against the system D-Bus bus and reported
one of nine states. This is the step most desktop tools skip, and it is why they are so often
unusable: an unprivileged process asking a root daemon to change USB authorization passes through
**three independent checkpoints**, each enforced by a different component, and each closed by
default in a stock configuration.

- **The D-Bus bus policy** decides which uids may send a message to `org.usbguard1` at all.
- **Polkit** checks an action per method call, against your session.
- **The daemon's own IPC access-control list** checks the process connected to its socket.

A failure at any one of them looks the same from the outside — "permission denied" — but the remedy
is different in each case. `--diagnose` tells you which one refused, and what to do about it. The
most common answer on a fresh system is neither of those three: it is that `usbguard-dbus` is not
installed, because no distribution installs it by default.

Follow whatever remedy it prints before continuing.

## 3. Open the window

```bash
usbguard-gui
```

You should see your USB devices with their current authorization state, updating live as you plug
and unplug things.

Expected output on the terminal:

```text
(nothing)
```

That is the intended result. The program logs at `warn` by default and has nothing to warn about.
If you want to see what it is doing:

```bash
USBGUARD_GUI_LOG=debug usbguard-gui
```

!!! danger "Do not paste a debug log into a public bug report"
    The `debug` level records device names, serial numbers, and hashes — an inventory of the
    hardware you own. Default-level logs deliberately contain none of that, which is why they are
    the ones to attach to an issue.

## 4. Clean Up

Nothing to undo. The program created no system state: it wrote no file outside your own
configuration, installed no Polkit rule, and changed nothing about your USB policy unless you
explicitly asked it to.

To remove the settings it stored for itself:

```bash
gsettings reset-recursively io.github.onyks_os.UsbguardGui
```

To remove the build:

```bash
make clean
```

If you *did* change your USB policy through the window and want it back the way it was, that is the
daemon's state, not this program's — use `usbguard` to inspect and revert it.

---

## Next Steps

- [How-To Guides](../how-to/index.md) — solve a specific problem.
- [Explanation: Architecture](../explanation/architecture.md) — understand the design.
- [Reference](../reference/cli.md) — the exhaustive option list.
