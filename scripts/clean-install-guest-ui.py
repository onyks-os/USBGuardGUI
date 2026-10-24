#!/usr/bin/env python3
"""Exercise the installed GUI and real Polkit agent inside a marked test VM."""
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT
import importlib
import json
import os
import subprocess
import tempfile
import time
from pathlib import Path

pyatspi = None

assert Path('/etc/usbguard-clean-install-vm').is_file()
assert os.getuid() != 0


def tree(root=None):
    root = pyatspi.Registry.getDesktop(0) if root is None else root
    yield root
    try:
        for index in range(min(root.childCount, 4096)):
            child = root.getChildAtIndex(index)
            if child is not None:
                yield from tree(child)
    except (RuntimeError, AttributeError):
        return


def wait(predicate, timeout=120):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.3)
    for item in tree():
        print('accessible:', item.getRoleName(), repr(item.name), flush=True)
    raise TimeoutError('Accessible element or device state not reached')


def find(name=None, role=None):
    for item in tree():
        try:
            if ((name is None or item.name == name) and (role is None or item.getRoleName() == role)
                    and (item.getRoleName() == 'application' or item.getState().contains(pyatspi.STATE_SHOWING))):
                return item
        except RuntimeError:
            continue
    return None


def click(item):
    actions = item.queryAction()
    if not actions.nActions:
        for child in tree(item):
            try:
                nested = child.queryAction()
                if nested.nActions:
                    actions = nested
                    break
            except NotImplementedError:
                continue
    if actions.nActions:
        assert actions.doAction(0)
    else:
        # GTK 4.14 MenuButton exports a non-actionable outer accessible.
        bounds = item.queryComponent().getExtents(pyatspi.DESKTOP_COORDS)
        assert bounds.width > 0 and bounds.height > 0
        subprocess.run(['xdotool', 'mousemove', str(bounds.x + bounds.width // 2),
                        str(bounds.y + bounds.height // 2), 'click', '1'], check=True)
    time.sleep(0.5)


def devices():
    return subprocess.check_output(['sudo', 'usbguard', 'list-devices'], text=True)


def target():
    lines = [line for line in devices().splitlines() if 'serial "usbguard-clean-test"' in line]
    assert len(lines) == 1, 'Only the synthetic fixture may be targeted'
    return lines[0].split(': ', 1)[1].split()[0]


processes = []
private_runtime = tempfile.TemporaryDirectory(prefix='usbguard-ui-', ignore_cleanup_errors=True)
display = f':{100 + os.getpid() % 60000}'
report = {'cases': [], 'os_release': Path('/etc/os-release').read_text(),
          'kernel': subprocess.check_output(['uname', '-r'], text=True).strip(),
          'usbguard_package': subprocess.check_output(
              ['dpkg-query', '-W', '-f=${Version}', 'usbguard'] if Path('/usr/bin/dpkg-query').exists() else
              ['rpm', '-q', 'usbguard'] if Path('/usr/bin/rpm').exists() else ['pacman', '-Q', 'usbguard'], text=True).strip(),
          'binary_sha256': subprocess.check_output(['sha256sum', '/usr/bin/usbguard-gui'], text=True).split()[0],
          'policy': 'Explicit remote-user fixture; real PAM AUTH_SELF, no cached authorization'}
try:
    log = Path('ui.log').open('w')  # noqa: SIM115 — closed after child cleanup
    env = dict(os.environ, DISPLAY=display, XDG_RUNTIME_DIR=private_runtime.name, GSETTINGS_BACKEND='memory', GSK_RENDERER='cairo',
               GTK_A11Y='atspi', LANG='C.UTF-8', LANGUAGE='en', NO_AT_BRIDGE='0')
    # AT-SPI reads DISPLAY in this Python process too.
    os.environ.update(env)
    processes.append(subprocess.Popen(['Xvfb', display, '-nolisten', 'tcp', '-screen', '0', '1280x900x24'], env=env, stdout=log, stderr=log))
    time.sleep(2)
    processes.append(subprocess.Popen(['openbox'], env=env, stdout=log, stderr=log))
    subprocess.run(['dbus-update-activation-environment', 'DISPLAY', 'XDG_RUNTIME_DIR', 'GSK_RENDERER', 'GTK_A11Y'], env=env, check=True)
    launcher = next(path for path in [Path('/usr/libexec/at-spi-bus-launcher'),
                    Path('/usr/lib/at-spi2-core/at-spi-bus-launcher'),
                    Path('/usr/lib/at-spi-bus-launcher')] if path.is_file())
    processes.append(subprocess.Popen([str(launcher), '--launch-immediately'], env=env, stdout=log, stderr=log))
    time.sleep(3)
    registry = next(path for path in [Path('/usr/libexec/at-spi2-registryd'),
                    Path('/usr/lib/at-spi2-core/at-spi2-registryd'),
                    Path('/usr/lib/at-spi2-registryd')] if path.is_file())
    processes.append(subprocess.Popen([str(registry)], env=env, stdout=log, stderr=log))
    time.sleep(2)
    pyatspi = importlib.import_module('pyatspi')
    app = subprocess.Popen(['usbguard-gui'], env=env, stdout=log, stderr=log)
    processes.append(app)
    wait(lambda: find(role='entry') or find(role='text'))
    print('GUI registered on accessibility bus', flush=True)
    if os.environ.get('USBGUI_DUMP_TREE'):
        time.sleep(10)
        for item in tree():
            print(item.getRoleName(), repr(item.name), flush=True)
    else:
        search = wait(lambda: find(role='entry') or find(role='text'))
        print('Search located', flush=True)
        search.queryEditableText().setTextContents('usbguard-clean-test')
        time.sleep(3)
        assert target() == 'allow'
        rules = subprocess.check_output(['sudo', 'usbguard', 'list-rules'], text=True)
        # With no agent the backend must refuse, leaving the GUI responsive.
        click(wait(lambda: find(name='Block')))
        subprocess.run(['xdotool', 'key', 'Return'], check=True)
        time.sleep(5)
        assert app.poll() is None and target() == 'allow'
        print('Checkpoint passed', flush=True)
        report['cases'].append('no-agent: refused; GUI alive; device unchanged')
        agent_paths = [Path('/usr/lib/policykit-1-gnome/polkit-gnome-authentication-agent-1'),
                       Path('/usr/libexec/polkit-gnome-authentication-agent-1'),
                       Path('/usr/libexec/polkit-mate-authentication-agent-1'),
                       Path('/usr/lib/mate-polkit/polkit-mate-authentication-agent-1'),
                       Path('/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1')]
        agent = next(path for path in agent_paths if path.is_file())
        processes.append(subprocess.Popen([str(agent)], env=env, stdout=log, stderr=log))
        time.sleep(4)
        click(wait(lambda: find(name='Block')))
        subprocess.run(['xdotool', 'key', 'Return'], check=True)
        wait(lambda: find(role='password text'))
        # Escape dismisses the real agent's dialog, like its Cancel button.
        subprocess.run(['xdotool', 'key', 'Escape'], check=True)
        time.sleep(4)
        assert app.poll() is None and target() == 'allow'
        print('Checkpoint passed', flush=True)
        report['cases'].append('agent-cancel: refused; GUI alive; device unchanged')
        click(wait(lambda: find(name='Block')))
        subprocess.run(['xdotool', 'key', 'Return'], check=True)
        password = wait(lambda: find(role='password text'))
        secret = json.loads(Path('credentials.json').read_text())['password']
        password.queryEditableText().setTextContents(secret)
        subprocess.run(['xdotool', 'key', 'Return'], check=True)
        wait(lambda: target() == 'block')
        assert app.poll() is None
        assert subprocess.check_output(['sudo', 'usbguard', 'list-rules'], text=True) == rules
        print('Checkpoint passed', flush=True)
        report['cases'].append('agent-password: runtime block; persistent rules unchanged')
        subprocess.run(['sudo', 'systemctl', 'restart', 'usbguard.service', 'usbguard-dbus.service'], check=True)
        wait(lambda: target() == 'allow')
        wait(lambda: find(name='Allowed'))
        assert app.poll() is None
        print('Checkpoint passed', flush=True)
        report['cases'].append('daemon-restart: GUI reconnected; persistent allow restored')
        report['passed'] = True
        Path('ui-report.json').write_text(json.dumps(report, indent=2) + '\n')
        print(json.dumps(report), flush=True)
finally:
    for process in reversed(processes):
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()

    log.close()
    private_runtime.cleanup()
