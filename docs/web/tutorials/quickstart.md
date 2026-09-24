# Tutorial: Quickstart

This tutorial takes you from nothing installed to allowing your first USB device from the window.
Follow it in order.

**You will need:** a Linux desktop session, administrator rights (for `sudo` and the password
prompts), a USB stick you can plug and unplug freely, and about 15 minutes.

---

## 1. Install

No release has been published yet, so build from source.

=== "Fedora / RHEL"

    ```bash
    sudo dnf install usbguard usbguard-dbus gtk4-devel libadwaita-devel gcc
    ```

=== "Debian / Ubuntu"

    ```bash
    sudo apt install usbguard libgtk-4-dev libadwaita-1-dev build-essential
    ```

=== "Arch"

    ```bash
    sudo pacman -S usbguard gtk4 libadwaita base-devel
    ```

Then, with a [Rust toolchain](https://rustup.rs/) (1.85 or newer):

```bash
git clone https://github.com/onyks-os/USBGuardGUI.git
cd USBGuardGUI
cargo build --release
```

The program is `target/release/usbguard-gui`. To install it as a package instead, run
`make package-deb` or `make package-rpm` and install the file from `dist/`.

## 2. Start USBGuard safely

!!! danger "An empty policy blocks your keyboard"
    USBGuard started without a policy blocks **every** USB device, including your keyboard and
    mouse. Always generate a policy that allows what is connected right now first.

Debian and Ubuntu already did this, and started the services, when the package was installed. On
Fedora and Arch:

```bash
sudo sh -c 'umask 077; usbguard generate-policy > /etc/usbguard/rules.conf'
sudo systemctl enable --now usbguard.service usbguard-dbus.service
```

The `umask 077` matters: the daemon refuses a rule file that other users can read.

## 3. Check that you can reach the daemon

```bash
target/release/usbguard-gui --diagnose
```

**What just happened:** the program checked, in order, that there is a system bus, that USBGuard's
D-Bus bridge is installed and running, and that you are allowed to read from the daemon. The last
line should be:

```text
Result: connected
```

If it is anything else, the lines below it name the cause and the command that fixes it. See
[Read a diagnostic result](../how-to/diagnose.md) for what each state means.

## 4. Open the window

```bash
target/release/usbguard-gui
```

You should see your USB devices, each with its state — *Allowed* or *Blocked* — shown by an icon
**and** a word, never by colour alone.

## 5. Allow a device for this session

1. Plug in the USB stick. With the policy generated in step 2 it is new to USBGuard, so it appears
   as **Blocked**, and a toast (or a notification, if the window is not in front) says so.
2. In its row, click **Allow**, then **This session only**.
3. Your system asks for the administrator password. Enter it.
4. The row changes to **Allowed** — only now, when USBGuard reports the change, never before.

**What just happened:** the program asked the daemon to authorize that device, and *this session
only* means the daemon keeps the decision in memory. When USBGuard restarts, the stick is blocked
again. Choosing **Permanently** instead writes a rule to the policy.

While the password prompt is open, the row shows a spinner and a **Cancel** button. Cancelling stops
the program from waiting; if the daemon had already acted, the list shows what it actually did.

## 6. Clean up

Unplug the stick. Nothing else to undo: the session-only decision disappears on its own when
USBGuard restarts, and the program created no system state — it installed no Polkit rule and wrote
no file outside your own settings.

To reset the settings the program stored for itself:

```bash
gsettings reset-recursively io.github.onyks_os.UsbguardGui
```

---

## Next Steps

- [How-To Guides](../how-to/index.md) — solve a specific problem.
- [Explanation: Architecture](../explanation/architecture.md) — understand the design.
- [Reference](../reference/cli.md) — every option and exit code.
