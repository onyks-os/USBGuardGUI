# USBGuard GUI (USBGuardGUI)

Unprivileged GTK4 desktop client for managing USB device authorization policy through the USBGuard daemon.

[Quickstart Guide](tutorials/quickstart.md){ .md-button .md-button--primary }
[Architecture](explanation/architecture.md){ .md-button }
[GitHub Repository](https://github.com/onyks-os/USBGuardGUI){ .md-button }

---

!!! warning "Not yet implemented"
    USBGuardGUI is specified but not written. There is no release, no binary, and nothing to
    install. This site documents the design so that it can be reviewed before it is built — read it
    as a plan, not as a manual. Progress is tracked in
    [ROADMAP.md](https://github.com/onyks-os/USBGuardGUI/blob/main/ROADMAP.md).

!!! info "It needs a package your distribution does not install by default"
    USBGuard's D-Bus bridge (`usbguard-dbus`) ships as a **separate package** on Fedora, Debian,
    Ubuntu, and Arch alike. Without it this program cannot reach the daemon at all. That is expected
    to be the single most common first-run outcome, which is why it is a named diagnostic state with
    a per-distribution install command rather than a generic error.

---

## Core Features

<div class="grid cards" markdown>

- **Unprivileged by construction**

    ---

    No setuid bit, no helper daemon, and no read or write of any file under `/etc`, `/var`, or
    `/sys`. Every privileged effect is produced by the USBGuard daemon on its own authority, after
    the system's own authorization layers have approved the request.

- **Failure is diagnosed, not reported**

    ---

    A probe sequence distinguishes nine distinct reasons the daemon can be unreachable — missing
    package, stopped service, denied by bus policy, denied by Polkit, denied by the IPC ACL, no
    Polkit agent — and names the specific remedy for each. Available headless as `--diagnose`.

- **Rule identity that survives concurrency**

    ---

    USBGuard rule IDs are positional and shift whenever the ruleset changes. Rules are held by
    canonical text and re-read immediately before use, so a rule deleted from another terminal
    reports an error instead of causing a different rule to be removed.

- **Live device view that absorbs bursts**

    ---

    Device state updates from daemon signals rather than polling. Insertion bursts are coalesced
    within a bounded latency window, so a 40-port hub plugged in at once does not freeze the
    interface or lose an event.

</div>

---

## Documentation Map

This site follows the [Diátaxis](https://diataxis.fr/) framework:

| Section | Answers | Read it when |
| :------ | :------ | :----------- |
| [Tutorials](tutorials/quickstart.md) | "Teach me the basics." | You are new. |
| [How-To Guides](how-to/index.md) | "How do I solve X?" | You have a specific goal. |
| [Explanation](explanation/architecture.md) | "Why is it built this way?" | You want to understand. |
| [Reference](reference/cli.md) | "What are the exact options?" | You need precise detail. |

---

## Project Status

**Pre-implementation.** No version has been released; the working version number is `0.1.0`. See
the [changelog](release-notes/changelog.md) and the
[roadmap](https://github.com/onyks-os/USBGuardGUI/blob/main/ROADMAP.md).

Security policy and vulnerability reporting:
[SECURITY.md](https://github.com/onyks-os/USBGuardGUI/blob/main/SECURITY.md).
