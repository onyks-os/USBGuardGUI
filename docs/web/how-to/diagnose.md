# Read a diagnostic result

`usbguard-gui --diagnose` — or the connection indicator in the window's header bar — reports one of
these states. A request to USBGuard crosses three independent checkpoints, each enforced by a
different component: the **D-Bus bus policy**, then **Polkit**, then **USBGuard's own IPC access
control**. The states say which one refused.

| Result | Exit code | Meaning | Remedy |
| :----- | :-------- | :------ | :----- |
| `connected` | 0 | Everything works. Writes are not tested in advance — the only test of a write is a write. | — |
| `no system D-Bus` | 5 | There is no system bus in this session — usually a container or sandbox. | Run on the host, or give the sandbox system-bus access. |
| `the USBGuard D-Bus bridge is not installed` | 4 | The package that provides `org.usbguard1` is missing. | [Install the D-Bus bridge](install-bridge.md). |
| `the USBGuard D-Bus bridge is not running` | 4 | Installed, but its service is stopped. | [Start it](install-bridge.md#if-it-is-not-running). |
| `denied by the D-Bus bus policy` | 3 | The bus refused the message before it reached USBGuard. A Polkit rule cannot fix this. | An administrator must change `org.usbguard1.conf`. |
| `denied by Polkit` | 3 | Polkit refused — or the password prompt was cancelled. | Try again, or see [Change policy with your own password](polkit-rule.md). |
| `no Polkit authentication agent is running` | 3 | A password was needed and nothing could ask for it. | Start your desktop's Polkit agent. |
| `denied by the USBGuard IPC access control` | 3 | USBGuard's own access control refused. The bridge normally satisfies it as root, so an administrator has narrowed it deliberately. | `sudo usbguard add-user …`, as `--diagnose` prints. |
| `access denied` | 3 | Refused, and the refusing layer could not be identified. | `--diagnose` lists every applicable remedy. |

Exit codes let scripts branch without parsing text:

```bash
usbguard-gui --diagnose >/dev/null
case $? in
  0) echo "ready" ;;
  4) echo "install or start usbguard-dbus" ;;
  3) echo "permissions" ;;
esac
```

!!! note "Why writes are never probed"
    Checking whether you may change the policy would mean changing the policy. The first real
    action you take carries that test, and a refusal is reported there, in its own context.
