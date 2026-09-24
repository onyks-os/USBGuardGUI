# Install the D-Bus bridge

USBGuardGUI reaches the USBGuard daemon only through its D-Bus bridge, `usbguard-dbus`, which owns
the name `org.usbguard1` on the system bus. `usbguard-gui --diagnose` tells two situations apart:

- **not installed** — nothing owns the name and nothing *can*: the package is missing;
- **not running** — the bridge is installed but its service is stopped.

## If it is not installed

Where the bridge lives depends on the distribution:

| Distribution | Package | Command |
| :----------- | :------ | :------ |
| Fedora, RHEL, CentOS | `usbguard-dbus` (separate) | `sudo dnf install usbguard-dbus` |
| Debian, Ubuntu | inside `usbguard` | `sudo apt install usbguard` |
| Arch | inside `usbguard` | `sudo pacman -S usbguard` |

## If it is not running

```bash
sudo systemctl enable --now usbguard-dbus.service
```

If `--diagnose` also reports that `usbguard.service` is not active, the daemon itself is stopped.
Before starting it for the first time, read the warning in the
[Quickstart](../tutorials/quickstart.md#2-start-usbguard-safely): a daemon started with an empty
policy blocks every USB device, keyboard included.

## Check

```bash
usbguard-gui --diagnose
```

The `Bridge (org.usbguard1)` line should read `running`, and the result `connected`.
