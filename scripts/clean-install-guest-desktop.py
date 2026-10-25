#!/usr/bin/env python3
"""Exercise the installed GUI in a private GNOME/Plasma Wayland guest session.

Requires the marked disposable Fedora VM and a private dbus-run-session. The
AT-SPI walk is limited to the exact GUI PID created by this test. Both desktop
components and the GUI use a software renderer and a virtual output.
"""
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT

import argparse
import datetime
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
from collections.abc import Callable, Iterator
from pathlib import Path
from typing import Any, TextIO, TypeVar

T = TypeVar("T")

spec = importlib.util.spec_from_file_location(
    "guest_wayland", Path(__file__).with_name("clean-install-guest-wayland.py")
)
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
APP = "io.github.onyks_os.UsbguardGui"
APP_PATH = "/io/github/onyks_os/UsbguardGui"
WATCHER = "org.kde.StatusNotifierWatcher"


def main() -> None:
    helper.require_guest()
    parser = argparse.ArgumentParser()
    parser.add_argument("--desktop", required=True, choices=("gnome", "kde"))
    args = parser.parse_args()
    receipt = Path("desktop-" + args.desktop + "-report.json")
    receipt.unlink(missing_ok=True)
    runtime = tempfile.TemporaryDirectory(
        prefix="usbguard-" + args.desktop + "-", ignore_cleanup_errors=True
    )
    env = dict(
        os.environ,
        XDG_RUNTIME_DIR=runtime.name,
        WAYLAND_DISPLAY="usbguard-desktop",
        GDK_BACKEND="wayland",
        GSK_RENDERER="cairo",
        GSETTINGS_BACKEND="dconf",
        GTK_A11Y="atspi",
        XDG_CONFIG_HOME=runtime.name + "/config",
        XDG_CACHE_HOME=runtime.name + "/cache",
        XDG_DATA_HOME=runtime.name + "/data",
        XDG_SESSION_TYPE="wayland",
        XDG_CURRENT_DESKTOP="GNOME" if args.desktop == "gnome" else "KDE",
        LIBGL_ALWAYS_SOFTWARE="1",
        QT_QUICK_BACKEND="software",
        LANG="C.UTF-8",
        LANGUAGE="en",
        NO_AT_BRIDGE="0",
    )
    env.pop("DISPLAY", None)
    os.environ.update(env)
    os.environ.pop("DISPLAY", None)
    processes = []
    required = []
    cases = []
    base = "desktop-" + args.desktop
    with (
        Path(base + "-shell.log").open("w") as shell_log,
        Path(base + "-app.log").open("w") as app_log,
        Path(base + "-notification.log").open("w") as notification_log,
    ):

        def spawn(
            command: list[str],
            log: TextIO = shell_log,
            critical: bool = False,
            extra_env: dict[str, str] | None = None,
        ) -> subprocess.Popen[bytes]:
            process = subprocess.Popen(
                command, env=extra_env or env, stdout=log, stderr=log
            )
            processes.append(process)
            if critical:
                required.append(process)
            return process

        def wait(predicate: Callable[[], T], timeout: float = 120) -> T:
            return helper.wait_for(predicate, required, timeout)

        try:
            subprocess.run(
                [
                    "dbus-update-activation-environment",
                    "WAYLAND_DISPLAY",
                    "XDG_RUNTIME_DIR",
                    "GDK_BACKEND",
                    "GSK_RENDERER",
                    "GTK_A11Y",
                    "GSETTINGS_BACKEND",
                    "XDG_CONFIG_HOME",
                    "XDG_CACHE_HOME",
                    "XDG_DATA_HOME",
                    "LIBGL_ALWAYS_SOFTWARE",
                    "QT_QUICK_BACKEND",
                    "XDG_CURRENT_DESKTOP",
                    "XDG_SESSION_TYPE",
                ],
                env=env,
                check=True,
            )
            if args.desktop == "gnome":
                for schema, key, value in (
                    ("org.gnome.desktop.interface", "enable-animations", "false"),
                    ("org.gnome.desktop.screensaver", "lock-enabled", "false"),
                    ("org.gnome.desktop.session", "idle-delay", "0"),
                ):
                    subprocess.run(
                        ["gsettings", "set", schema, key, value], env=env, check=True
                    )
            launcher = next(
                p
                for p in (
                    Path("/usr/libexec/at-spi-bus-launcher"),
                    Path("/usr/lib/at-spi2-core/at-spi-bus-launcher"),
                )
                if p.is_file()
            )
            spawn([str(launcher), "--launch-immediately"])
            command = (
                [
                    "gnome-shell",
                    "--headless",
                    "--wayland",
                    "--no-x11",
                    "--virtual-monitor=1280x900",
                    "--wayland-display=usbguard-desktop",
                ]
                if args.desktop == "gnome"
                else [
                    "kwin_wayland",
                    "--virtual",
                    "--width",
                    "1280",
                    "--height",
                    "900",
                    "--no-lockscreen",
                    "--socket",
                    "usbguard-desktop",
                ]
            )
            spawn(command, critical=True)
            wait(lambda: (Path(runtime.name) / "usbguard-desktop").is_socket(), 180)
            registry = next(
                p
                for p in (
                    Path("/usr/libexec/at-spi2-registryd"),
                    Path("/usr/lib/at-spi2-core/at-spi2-registryd"),
                )
                if p.is_file()
            )
            spawn([str(registry)])
            import gi

            gi.require_version("Gio", "2.0")
            from gi.repository import Gio, GLib
            import pyatspi

            bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)

            def call(
                destination: str,
                path: str,
                interface: str,
                method: str,
                parameters: Any = None,
            ) -> tuple[Any, ...]:
                return bus.call_sync(
                    destination,
                    path,
                    interface,
                    method,
                    parameters,
                    None,
                    Gio.DBusCallFlags.NONE,
                    15000,
                    None,
                ).unpack()

            def owned(name: str) -> bool:
                return call(
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "NameHasOwner",
                    GLib.Variant("(s)", (name,)),
                )[0]

            def prop(destination: str, path: str, interface: str, name: str) -> Any:
                return call(
                    destination,
                    path,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    GLib.Variant("(ss)", (interface, name)),
                )[0]

            if args.desktop == "kde":
                spawn(["kded6"])
                spawn(["plasmashell"], critical=True)
                wait(
                    lambda: (
                        owned(WATCHER)
                        and prop(
                            WATCHER,
                            "/StatusNotifierWatcher",
                            WATCHER,
                            "IsStatusNotifierHostRegistered",
                        )
                    ),
                    180,
                )
            else:
                assert not owned(WATCHER), (
                    "Stock GNOME fixture unexpectedly has a status notifier watcher"
                )
                wait(
                    lambda: (
                        owned("org.freedesktop.Notifications")
                        or owned("org.gtk.Notifications")
                    ),
                    180,
                )
            subprocess.run(
                ["gsettings", "set", APP, "run-in-background", "false"],
                env=env,
                check=True,
            )
            app = spawn(
                ["usbguard-gui"],
                log=app_log,
                critical=True,
                extra_env=dict(env, WAYLAND_DEBUG="client"),
            )
            frame = wait(
                lambda: helper.mapped_frame(
                    Path(base + "-app.log").read_text(errors="replace")
                ),
                180,
            )
            cases.append(
                "native Wayland: main surface configured, buffer committed and frame received"
            )
            print("PASS checkpoint:", cases[-1], flush=True)

            def walk(root: Any) -> Iterator[Any]:
                yield root
                try:
                    for index in range(min(root.childCount, 4096)):
                        child = root.getChildAtIndex(index)
                        if child is not None:
                            yield from walk(child)
                except (RuntimeError, AttributeError):
                    pass

            def roots() -> Iterator[Any]:
                desktop = pyatspi.Registry.getDesktop(0)
                for index in range(desktop.childCount):
                    root = desktop.getChildAtIndex(index)
                    if root and root.get_process_id() == app.pid:
                        yield root

            def find(
                name: str | None = None,
                role: str | tuple[str, ...] | None = None,
                root: Any = None,
                prefix: bool = False,
            ) -> Any:
                for application in [root] if root is not None else roots():
                    for item in walk(application):
                        try:
                            if (
                                (
                                    name is None
                                    or (
                                        item.name.startswith(name)
                                        if prefix
                                        else item.name == name
                                    )
                                )
                                and (
                                    role is None
                                    or item.getRoleName()
                                    in ((role,) if isinstance(role, str) else role)
                                )
                                and item.getState().contains(pyatspi.STATE_SHOWING)
                            ):
                                return item
                        except RuntimeError:
                            pass
                return None

            def click(item: Any) -> None:
                # Prefer the actual control to an enclosing accessible row or
                # label that may advertise an unrelated activation action.
                for candidate in reversed(list(walk(item))):
                    try:
                        action = candidate.queryAction()
                        if action.nActions:
                            assert action.doAction(0)
                            return
                    except NotImplementedError:
                        pass
                raise RuntimeError(
                    "Accessible widget has no action: " + repr(item.name)
                )

            search = wait(lambda: find(name="Filter devices by name, ID, or serial"))
            search.queryEditableText().setTextContents("usbguard-clean-test")
            wait(lambda: find(name="Allowed"))
            for name in ("Connected USB devices", "Show device details", "Main menu"):
                wait(lambda name=name: find(name=name))
            cases.append(
                "AT-SPI: named device controls exposed; synthetic device filter responds"
            )
            print("PASS checkpoint:", cases[-1], flush=True)
            # Open the same action offered by the main menu; the dialog is native GTK.
            call(
                APP,
                APP_PATH + "/window/1",
                "org.gtk.Actions",
                "Activate",
                GLib.Variant("(sava{sv})", ("preferences", [], {})),
            )
            background = wait(
                lambda: find(name="Keep running in the background", role="switch")
            )

            def background_enabled() -> bool:
                return any(
                    item.getState().contains(pyatspi.STATE_CHECKED)
                    for item in walk(background)
                )

            for widget in walk(background):
                if widget.getRoleName() == "switch":
                    actions = widget.queryAction()
                    print(
                        "Background control:",
                        widget.getRoleName(),
                        repr(widget.name),
                        "states",
                        widget.getState().getStates(),
                        "actions",
                        [actions.getName(i) for i in range(actions.nActions)],
                        flush=True,
                    )
            if not background_enabled():
                click(background)
            wait(background_enabled, 15)
            stored = subprocess.check_output(
                ["gsettings", "get", APP, "run-in-background"], env=env, text=True
            ).strip()
            assert stored == "true", "Background preference did not persist: " + stored
            print(
                "Background preference enabled through the native UI and stored",
                flush=True,
            )
            dialog = wait(lambda: find(role="dialog"))
            close = wait(
                lambda: find(name="Close", role=("button", "push button"), root=dialog)
            )
            click(close)
            wait(lambda: not find(role="dialog"))
            cases.append(
                "preferences: native dialog opens and closes without losing the main window"
            )
            print("PASS checkpoint:", cases[-1], flush=True)
            owner = call(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetNameOwner",
                GLib.Variant("(s)", (APP,)),
            )[0]
            matches = [
                f"type='method_call',sender='{owner}',interface='{iface}'"
                for iface in (
                    "org.gtk.Notifications",
                    "org.freedesktop.Notifications",
                    "org.freedesktop.portal.Notification",
                )
            ]
            spawn(
                ["stdbuf", "-oL", "dbus-monitor", "--session", *matches],
                log=notification_log,
            )
            time.sleep(1)
            tray_service = f"org.kde.StatusNotifierItem-{app.pid}-1"
            if args.desktop == "kde":
                wait(
                    lambda: (
                        tray_service + "/StatusNotifierItem"
                        in prop(
                            WATCHER,
                            "/StatusNotifierWatcher",
                            WATCHER,
                            "RegisteredStatusNotifierItems",
                        )
                    )
                )
                assert (
                    prop(
                        tray_service,
                        "/StatusNotifierItem",
                        "org.kde.StatusNotifierItem",
                        "Title",
                    )
                    == "USBGuardGUI"
                )
            click(wait(lambda: find(name="Close", role=("button", "push button"))))
            wait(lambda: not find(role="frame"))
            expected = (
                "Use the status icon to reopen it"
                if args.desktop == "kde"
                else "This desktop has no status icon area"
            )
            wait(
                lambda: (
                    expected
                    in Path(base + "-notification.log").read_text(errors="replace")
                )
            )
            assert app.poll() is None

            def notification_count() -> int:
                return len(
                    re.findall(
                        r"member=(?:AddNotification|Notify)(?:\s|$)",
                        Path(base + "-notification.log").read_text(errors="replace"),
                        re.MULTILINE,
                    )
                )

            assert notification_count() == 1
            cases.append(
                "background: close hides the window, keeps GUI alive and sends the correct one-time notice"
            )
            print("PASS checkpoint:", cases[-1], flush=True)
            second = spawn(["usbguard-gui"], log=app_log)
            assert second.wait(timeout=60) == 0
            wait(lambda: find(role="frame"))
            assert app.poll() is None
            assert (
                owner
                == call(
                    "org.freedesktop.DBus",
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "GetNameOwner",
                    GLib.Variant("(s)", (APP,)),
                )[0]
            )
            cases.append("single instance: relaunch reopens the original process")
            print("PASS checkpoint:", cases[-1], flush=True)
            click(wait(lambda: find(name="Close", role=("button", "push button"))))
            wait(lambda: not find(role="frame"))
            time.sleep(2)
            assert notification_count() == 1, (
                "Background notice repeated on the second close"
            )
            if args.desktop == "kde":
                call(
                    tray_service,
                    "/StatusNotifierItem",
                    "org.kde.StatusNotifierItem",
                    "Activate",
                    GLib.Variant("(ii)", (0, 0)),
                )
                wait(lambda: find(role="frame"))
                call(
                    tray_service,
                    "/MenuBar",
                    "com.canonical.dbusmenu",
                    "Event",
                    GLib.Variant("(isvu)", (3, "clicked", GLib.Variant("s", ""), 0)),
                )
                assert app.wait(timeout=60) == 0
                cases.append(
                    "Plasma tray: real watcher registration, Activate reopens, Quit menu exits cleanly"
                )
            else:
                call(
                    APP,
                    APP_PATH,
                    "org.gtk.Actions",
                    "Activate",
                    GLib.Variant("(sava{sv})", ("quit", [], {})),
                )
                assert app.wait(timeout=60) == 0
                assert not owned(WATCHER)
                cases.append("GNOME: background without a watcher; Quit exits cleanly")
            required.remove(app)
            print("PASS checkpoint:", cases[-1], flush=True)
            report = {
                "passed": True,
                "desktop": args.desktop,
                "validated_at": datetime.datetime.now(datetime.UTC).isoformat(),
                "scope": "Private headless native-package desktop session, software rendering, synthetic USB only",
                "os_release": Path("/etc/os-release").read_text(),
                "kernel": subprocess.check_output(["uname", "-r"], text=True).strip(),
                "binary_sha256": hashlib.sha256(
                    Path("/usr/bin/usbguard-gui").read_bytes()
                ).hexdigest(),
                "cases": cases,
                "window": frame,
                "settings_backend": "dconf in private XDG directories and session bus",
                "display_size": [1280, 900],
                "compositor_version": subprocess.run(
                    [command[0], "--version"],
                    env=env,
                    check=False,
                    capture_output=True,
                    text=True,
                ).stdout.strip(),
                "selinux": subprocess.check_output(["getenforce"], text=True).strip(),
            }
        except Exception:
            for filename in (
                base + "-shell.log",
                base + "-app.log",
                base + "-notification.log",
            ):
                print(
                    filename
                    + ":\n"
                    + Path(filename).read_text(errors="replace")[-6000:],
                    flush=True,
                )
            # Only this synthetic app's tree is printed for diagnosis.
            if "roots" in locals():
                for application in roots():
                    for item in walk(application):
                        try:
                            print(
                                "APP accessible:",
                                item.getRoleName(),
                                repr(item.name),
                                flush=True,
                            )
                        except RuntimeError:
                            pass
            raise
        finally:
            for process in reversed(processes):
                if process.poll() is None:
                    process.terminate()
                    try:
                        process.wait(timeout=15)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            mount = Path(runtime.name) / "doc"
            if os.path.ismount(mount):
                unmount = shutil.which("fusermount3") or shutil.which("fusermount")
                if unmount:
                    subprocess.run([unmount, "-uz", str(mount)], check=True)
            runtime.cleanup()
    report["driver_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    report["wayland_helper_sha256"] = hashlib.sha256(
        Path(__file__).with_name("clean-install-guest-wayland.py").read_bytes()
    ).hexdigest()
    report["protocol_sha256"] = hashlib.sha256(
        Path(base + "-app.log").read_bytes()
    ).hexdigest()
    receipt.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2), flush=True)


if __name__ == "__main__":
    main()
