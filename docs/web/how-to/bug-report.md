# Report a bug without leaking your hardware

The most useful thing to attach to an issue is the output of:

```bash
usbguard-gui --diagnose
```

It contains versions and access states, and no device information.

## Logs

The program logs to the terminal it was started from, at the `warn` level by default. To get more:

```bash
USBGUARD_GUI_LOG=info usbguard-gui
```

!!! danger "Never paste a `debug` log into a public issue"
    At `debug` level and below, the log records device names, serial numbers, and hashes — an
    inventory of the hardware you own. Logs at `info` and above contain none of that, which is why
    they are the ones to share.

## Security issues

Do not open a public issue for a vulnerability. Follow
[SECURITY.md](https://github.com/onyks-os/USBGuardGUI/blob/main/SECURITY.md).
