# Change policy with your own password

By default, USBGuard lets any active local user **read** devices and rules, and asks for the
**administrator** password for every **change**. The package ships an example Polkit rule that lets
members of the administrator group confirm changes with their **own** password instead, remembered
for a few minutes, and read without a prompt.

!!! warning "This widens who may change USB policy"
    The rule is shipped as documentation and **never installed** by the package. Installing it is an
    administrator's decision about the system's security. Read it first:

    ```bash
    cat /usr/share/doc/usbguard-gui/70-usbguard-gui.rules.example
    ```

It applies only to a **local, active** session: a remote SSH session keeps the default.

## Install it

Check the group name in the file: `wheel` on Fedora and Arch, `sudo` on Debian and Ubuntu. Edit it
if needed, then:

```bash
sudo install -m 0644 /usr/share/doc/usbguard-gui/70-usbguard-gui.rules.example \
     /etc/polkit-1/rules.d/70-usbguard-gui.rules
```

`install -m 0644` rather than `cp`: Polkit runs as its own user and silently ignores a rule file it
cannot read. Polkit picks up the new rule immediately; nothing needs restarting.

## Remove it

```bash
sudo rm /etc/polkit-1/rules.d/70-usbguard-gui.rules
```
