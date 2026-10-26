#!/usr/bin/env python3
"""Verify a rendered native Wayland window only inside a marked disposable VM.

Run with dbus-run-session after installing Weston in the guest. This is a
Weston/Pixman smoke test; it does not replace GNOME/KDE or accessibility checks.
"""
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT
import datetime
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
from collections.abc import Callable
from pathlib import Path
from typing import TypeVar

T = TypeVar('T')


def require_guest(marker: Path = Path('/etc/usbguard-clean-install-vm')) -> None:
    if not marker.is_file() or os.getuid() == 0:
        raise RuntimeError('Requires the unprivileged user inside the marked disposable VM')
    if json.loads(marker.read_text()).get('serial') != 'usbguard-clean-test':
        raise RuntimeError('The marked VM must contain the synthetic USB fixture')


def mapped_frame(protocol: str) -> dict[str, int] | None:
    """Require configuration, a committed buffer and its frame callback together.

    Registry traffic, an empty initial commit, a cursor buffer, and an unrelated
    callback must not turn an incomplete launch into a passing window test.
    """
    for created in re.finditer(
            r'get_xdg_surface\(new id xdg_surface#(\d+), wl_surface#(\d+)\)', protocol):
        xdg, surface = created.groups()
        events = protocol[created.end():]
        toplevel = re.search(rf'xdg_surface#{xdg}\.get_toplevel\(new id xdg_toplevel#(\d+)\)', events)
        if not toplevel:
            continue
        title = re.search(rf'xdg_toplevel#{toplevel[1]}\.set_title\("USBGuardGUI"\)', events)
        configured = re.search(rf'xdg_surface#{xdg}\.ack_configure\(\d+\)', events)
        if not title or not configured:
            continue
        configured_end = configured.end()
        start = configured_end
        for committed in re.finditer(rf'wl_surface#{surface}\.commit\(\)', events[configured_end:]):
            end = configured_end + committed.end()
            pending = events[start:end]
            start = end
            attached = re.search(rf'wl_surface#{surface}\.attach\(wl_buffer#\d+,', pending)
            frame = re.search(rf'wl_surface#{surface}\.frame\(new id wl_callback#(\d+)\)', pending)
            if attached and frame and re.search(rf'wl_callback#{frame[1]}\.done\(\d+\)', events[end:]):
                return {'xdg_surface': int(xdg), 'surface': int(surface),
                        'toplevel': int(toplevel[1]), 'frame_callback': int(frame[1])}
    return None


def wait_for(predicate: Callable[[], T], processes: list[subprocess.Popen[bytes]], timeout: float) -> T:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for process in processes:
            if process.poll() is not None:
                raise RuntimeError(f'Test process exited prematurely: {process.args[0]} ({process.returncode})')
        value = predicate()
        if value:
            return value
        time.sleep(0.25)
    raise TimeoutError(f'Wayland checkpoint not reached within {timeout} seconds')


def main() -> None:
    require_guest()
    receipt = Path('wayland-report.json')
    receipt.unlink(missing_ok=True)
    for executable in ('weston', 'usbguard-gui', 'dbus-update-activation-environment'):
        if not shutil.which(executable):
            raise RuntimeError(f'Install {executable} inside the disposable guest before this check')
    if not os.environ.get('DBUS_SESSION_BUS_ADDRESS'):
        raise RuntimeError('Run this guest check with dbus-run-session')
    help_text = subprocess.check_output(['weston', '--help'], text=True, stderr=subprocess.STDOUT)
    if '--fake-seat' not in help_text:
        raise RuntimeError('This headless smoke test requires Weston with --fake-seat support')

    runtime = tempfile.TemporaryDirectory(prefix='usbguard-wayland-', ignore_cleanup_errors=True)
    processes = []
    env = dict(os.environ, XDG_RUNTIME_DIR=runtime.name, WAYLAND_DISPLAY='usbguard-wayland',
               GDK_BACKEND='wayland', GSK_RENDERER='cairo', GSETTINGS_BACKEND='memory', GTK_A11Y='none',
               XDG_CONFIG_HOME=runtime.name + '/config', LANG='C.UTF-8', LANGUAGE='en')
    env.pop('DISPLAY', None)
    try:
        with Path('wayland-compositor.log').open('w') as compositor_log, Path('wayland-app.log').open('w') as app_log:
            compositor = subprocess.Popen(
                ['weston', '--no-config', '--backend=headless', '--renderer=pixman', '--fake-seat',
                 '--socket=usbguard-wayland', '--idle-time=0'], env=env, stdout=compositor_log, stderr=compositor_log)
            processes.append(compositor)
            wait_for(lambda: (Path(runtime.name) / 'usbguard-wayland').is_socket(), processes, 30)
            subprocess.run(['dbus-update-activation-environment', 'WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR',
                            'GDK_BACKEND', 'GSK_RENDERER', 'GTK_A11Y', 'XDG_CONFIG_HOME'], env=env, check=True)
            launched_at = time.monotonic()
            app = subprocess.Popen(['usbguard-gui'], env=dict(env, WAYLAND_DEBUG='client'), stdout=app_log, stderr=app_log)
            processes.append(app)
            evidence = wait_for(lambda: mapped_frame(Path('wayland-app.log').read_text(errors='replace')), processes, 120)
            mapped_after = time.monotonic() - launched_at
            # Observe five more seconds; a mapped window followed by a crash fails.
            observed_at = time.monotonic()
            wait_for(lambda: time.monotonic() - observed_at >= 5, processes, 10)
            report = {
                'passed': True, 'validated_at': datetime.datetime.now(datetime.UTC).isoformat(),
                'scope': 'Native Wayland window on headless Weston/Pixman with a synthetic seat; no X11 fallback',
                'os_release': Path('/etc/os-release').read_text(),
                'kernel': subprocess.check_output(['uname', '-r'], text=True).strip(),
                'compositor': subprocess.check_output(['weston', '--version'], text=True).strip(),
                'binary_sha256': hashlib.sha256(Path('/usr/bin/usbguard-gui').read_bytes()).hexdigest(),
                'window': evidence, 'mapped_after_seconds': round(mapped_after, 3), 'alive_after_frame_seconds': 5,
                'cases': ['main toplevel configured', 'main surface buffer committed',
                          'matching frame callback received', 'GUI alive for five seconds after frame'],
            }
    except Exception:
        for filename in ('wayland-compositor.log', 'wayland-app.log'):
            if Path(filename).is_file():
                print(filename + ':\n' + Path(filename).read_text(errors='replace')[-6000:], flush=True)
        raise
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
        mount = Path(runtime.name) / 'doc'
        if os.path.ismount(mount):
            unmount = shutil.which('fusermount3') or shutil.which('fusermount')
            if unmount:
                subprocess.run([unmount, '-uz', str(mount)], check=True)
        runtime.cleanup()
    report['test_driver_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    report['protocol_sha256'] = hashlib.sha256(Path('wayland-app.log').read_bytes()).hexdigest()
    receipt.write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2), flush=True)


if __name__ == '__main__':
    main()
