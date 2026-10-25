#!/bin/bash
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT
# Explicit Polkit fixture for a remote disposable test session, never packaged.
set -euo pipefail
test -f /etc/usbguard-clean-install-vm
test "$(id -u)" = 0
mkdir -p /etc/polkit-1/rules.d
cat > /etc/polkit-1/rules.d/00-usbguard-clean-install-test.rules <<'RULE'
polkit.addRule(function(action, subject) {
    if (subject.user !== "usbguard-test") return;
    var readOnly = ["org.usbguard.Devices1.listDevices", "org.usbguard.Policy1.listRules", "org.usbguard1.getParameter"];
    var writes = ["org.usbguard.Devices1.applyDevicePolicy", "org.usbguard.Policy1.appendRule", "org.usbguard.Policy1.removeRule", "org.usbguard1.setParameter"];
    if (readOnly.indexOf(action.id) >= 0) return polkit.Result.YES;
    if (writes.indexOf(action.id) >= 0) return polkit.Result.AUTH_SELF;
});
RULE
chmod 0644 /etc/polkit-1/rules.d/00-usbguard-clean-install-test.rules
systemctl restart polkit.service
