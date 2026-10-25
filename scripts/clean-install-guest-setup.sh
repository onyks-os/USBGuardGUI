#!/bin/bash
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT
# Run only inside a disposable VM prepared by verify-clean-install.py.
set -euo pipefail
if [[ ! -f /etc/usbguard-clean-install-vm ]] || [[ $(id -u) != 0 ]]; then
    echo 'Requires root inside the marked disposable VM' >&2
    exit 1
fi
# shellcheck disable=SC1091
. /etc/os-release
case "$ID" in
    ubuntu|debian)
        apt-get update
        agent=policykit-1-gnome
        if [[ "$ID" == debian ]]; then agent=mate-polkit; fi
        DEBIAN_FRONTEND=noninteractive apt-get install -y usbguard usbutils xvfb openbox \
            "$agent" python3-pyatspi python3-pexpect dbus-x11 \
            libgtk-4-1 libadwaita-1-0 xdotool
        ;;
    fedora)
        dnf -y install usbguard usbguard-dbus usbutils xorg-x11-server-Xvfb openbox \
            mate-polkit python3-pyatspi python3-pexpect dbus-x11 gtk4 libadwaita xdotool
        ;;
    arch)
        pacman -Syu --noconfirm --needed usbguard usbutils xorg-server-xvfb openbox \
            polkit-gnome python-atspi python-pexpect dbus gtk4 libadwaita xdotool
        ;;
    *) echo "Unsupported guest: $ID" >&2; exit 1 ;;
esac
# Preserve the distribution defaults before configuring the synthetic fixture.
mkdir -p /home/usbguard-test/baseline
cp /etc/usbguard/usbguard-daemon.conf /home/usbguard-test/baseline/
for busdir in /usr/share/dbus-1/system.d /etc/dbus-1/system.d; do
    [[ -d "$busdir" ]] || continue
    find "$busdir" -name '*usbguard*' -exec cp '{}' /home/usbguard-test/baseline/ \;
done
find /usr/share/polkit-1/actions -name '*usbguard*' -exec cp '{}' /home/usbguard-test/baseline/ \;
chown -R usbguard-test: /home/usbguard-test/baseline
modprobe xhci_pci
lsusb > /home/usbguard-test/baseline/lsusb.txt
# The fixture contains only a synthetic QEMU device. Keep policy changes local
# to this explicitly marked VM; never grant the GUI additional privileges.
usbguard generate-policy > /etc/usbguard/rules.conf
# Newly installed socket units are not started by an already-reached target.
# Modern Polkit uses this instead of a setuid helper.
if systemctl cat polkit-agent-helper.socket >/dev/null 2>&1; then
    systemctl start polkit-agent-helper.socket
fi
systemctl enable --now usbguard.service
systemctl enable --now usbguard-dbus.service
# Reload the fixture even when package installation already started the daemon.
systemctl restart usbguard.service usbguard-dbus.service
chown -R usbguard-test: /home/usbguard-test/baseline
usbguard list-devices
