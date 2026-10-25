<!--
Copyright (c) 2026 onyks-os
SPDX-License-Identifier: MIT
-->

# ADR 0002: Export the fixed status notifier and menu on zbus

## Status

Accepted (post-0.1.2)

## Context

zbus 6 incorporates zvariant, zvariant_derive and zbus_names, whose standalone
crates have unmaintained advisories. Its Rust minimum is 1.89. The published
ksni 0.3.7 release still requires zbus 5, so updating only the direct dependency
would retain the deprecated stack through the tray. Architecture §9.6 already
allows a direct implementation as an alternative.

## Decision

Use zbus 6.1 for the USBGuard proxies and the fixed StatusNotifierItem and
com.canonical.dbusmenu interfaces. Export Show, a separator and Quit; preserve
the GTK command channel and counts-only state boundary. Register a standard
service name natively and the unique connection name inside Flatpak. Re-register
the item when the watcher restarts. Emit both tooltip protocol signals and
PropertiesChanged.

Raise the manifest/CI minimum and pinned reproducible builder to Rust 1.89.
Use serde derives for menu serialization. Keep all dependencies on crates.io;
do not add a temporary Git dependency or maintain a ksni fork.

### Alternatives considered

| Alternative | Reason not selected |
| :---------- | :------------------ |
| Wait for a compatible ksni release | Keeps the deprecated stack for an external, uncommitted release date. |
| Upgrade only the direct zbus dependency | Retains zbus 5 and deprecated types through ksni. |
| Depend on the upstream ksni migration PR | The migration is not a published, validated release. |

## Consequences

The project owns a small, fixed protocol surface and its compatibility tests.
Private-bus tests cover signatures, properties, menu actions, tooltip signals,
native/sandbox registration, missing hosts, watcher replacement and shutdown.
No GTK object crosses into Tokio. There is no new privileged code in the app.
Flatpak hosts require xdg-dbus-proxy 0.1.6 or newer, following zbus 6's requirements.

## References

- [Architecture §9.6](../architecture.md#96-system-tray-and-background-mode)
- [Dependency inventory](../../DEPENDENCIES.md)
- [Upstream zbus 6 migration guide](https://github.com/z-galaxy/zbus/blob/main/book/src/upgrading-to-6.md)
- [Upstream ksni migration PR](https://github.com/iovxw/ksni/pull/45)
