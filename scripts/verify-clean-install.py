#!/usr/bin/env python3
"""Manage disposable rootless QEMU guests for real USBGuard/Polkit checks.

All writable disks, SSH credentials, logs and cloud-init data stay in --output.
The VM sees a synthetic USB disk, never a host USB device or host filesystem.
"""
# Copyright (c) 2026 onyks-os
# SPDX-License-Identifier: MIT

import argparse
import hashlib
import json
import secrets
import shlex
import socket
import subprocess
import time
from pathlib import Path

USER = "usbguard-test"
SERIAL = "usbguard-clean-test"


def command(args, **kwargs):
    return subprocess.run(args, check=True, **kwargs)


def ssh_args(directory):
    data = json.loads((directory / "vm.json").read_text())
    return ["ssh", "-i", str(directory / "identity"), "-p", str(data["port"]),
            "-o", "IdentitiesOnly=yes", "-o", "BatchMode=yes", "-o", "ConnectTimeout=5",
            "-o", "StrictHostKeyChecking=accept-new", "-o",
            f"UserKnownHostsFile={directory / 'known_hosts'}", f"{USER}@127.0.0.1"]


def prepare(args):
    image, directory = args.image.resolve(), args.output.resolve()
    with image.open("rb") as source:
        actual = hashlib.file_digest(source, "sha256").hexdigest()
    if actual != args.sha256:
        raise ValueError("Cloud image SHA-256 mismatch")
    directory.mkdir(parents=True, exist_ok=False)
    directory.chmod(0o700)
    command(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(directory / "identity")])
    password = secrets.token_urlsafe(18)
    hashed = subprocess.check_output(["openssl", "passwd", "-6", "-stdin"], input=password + "\n", text=True).strip()
    group = "sudo" if args.distro in ("ubuntu", "debian") else "wheel"
    config = {"hostname": "usbguard-clean-install", "ssh_pwauth": False,
              "users": [{"name": USER, "groups": [group], "shell": "/bin/bash",
                         "sudo": ["ALL=(ALL) NOPASSWD:ALL"], "lock_passwd": False,
                         "passwd": hashed, "ssh_authorized_keys": [(directory / "identity.pub").read_text().strip()]}],
              "write_files": [{"path": "/etc/usbguard-clean-install-vm", "permissions": "0644",
                               "content": json.dumps({"distro": args.distro, "serial": SERIAL}) + "\n"}]}
    (directory / "user-data").write_text("#cloud-config\n" + json.dumps(config, indent=2) + "\n")
    (directory / "meta-data").write_text("instance-id: usbguard-" + secrets.token_hex(8) + "\n")
    credentials = directory / "credentials.json"
    credentials.write_text(json.dumps({"password": password}))
    credentials.chmod(0o600)
    command(["genisoimage", "-quiet", "-output", str(directory / "seed.iso"), "-volid", "cidata",
             "-joliet", "-rock", str(directory / "user-data"), str(directory / "meta-data")])
    command(["qemu-img", "create", "-q", "-f", "qcow2", "-F", "qcow2", "-b", str(image),
             str(directory / "disk.qcow2"), "12G"])
    with (directory / "usb.raw").open("wb") as usb:
        usb.truncate(16 * 1024 * 1024)
    with socket.socket() as allocation:
        allocation.bind(("127.0.0.1", 0))
        port = allocation.getsockname()[1]
    (directory / "vm.json").write_text(json.dumps({"distro": args.distro, "image": str(image),
        "image_sha256": actual, "port": port, "serial": SERIAL, "acceleration": "tcg"}, indent=2) + "\n")
    qemu = ["qemu-system-x86_64", "-machine", "q35", "-accel", "tcg,thread=multi", "-cpu", "max",
            "-smp", "2", "-m", "2048", "-display", "none", "-daemonize",
            "-pidfile", str(directory / "qemu.pid"), "-serial", f"file:{directory / 'console.log'}",
            "-qmp", f"unix:{directory / 'qmp.sock'},server=on,wait=off",
            "-drive", f"file={directory / 'disk.qcow2'},format=qcow2,if=virtio",
            "-drive", f"file={directory / 'seed.iso'},format=raw,media=cdrom,readonly=on",
            "-netdev", f"user,id=net,hostfwd=tcp:127.0.0.1:{port}-:22", "-device", "virtio-net-pci,netdev=net",
            "-device", "qemu-xhci,id=usbcontroller", "-drive", f"if=none,id=usbtest,file={directory / 'usb.raw'},format=raw",
            "-device", f"usb-storage,drive=usbtest,serial={SERIAL},id=usbtest-device"]
    command(qemu)
    print(f"Booting isolated {args.distro} VM; SSH port {port}", flush=True)
    deadline = time.monotonic() + 1200
    with (directory / "ssh-startup.log").open("w") as log:
        while time.monotonic() < deadline:
            result = subprocess.run([*ssh_args(directory), "true"], stdout=log, stderr=log, check=False)
            if result.returncode == 0:
                break
            time.sleep(5)
        else:
            raise TimeoutError(f"Guest did not boot; inspect {directory / 'console.log'}")
    command([*ssh_args(directory), "sudo cloud-init status --wait"])
    print(f"Clean VM ready: {directory}", flush=True)


def validate(args):
    directory = args.output.resolve()
    marker = json.loads((directory / "vm.json").read_text())
    package = args.package.resolve()
    extension = ".deb" if marker["distro"] in ("ubuntu", "debian") else ".rpm" if marker["distro"] == "fedora" else ".pkg.tar.zst"
    if not package.name.endswith(extension):
        raise ValueError("Package format does not match the guest")
    uploads = [(package, "candidate" + extension),
               (Path(__file__).with_name("clean-install-guest-policy.sh"), "policy.sh"),
               (Path(__file__).with_name("clean-install-guest-ui.py"), "ui-test.py"),
               (directory / "credentials.json", "credentials.json")]
    for source, name in uploads:
        with source.open("rb") as content:
            command([*ssh_args(directory), "umask 077; cat > " + shlex.quote(f"/home/{USER}/{name}")], stdin=content)
    install = {"ubuntu": "sudo apt-get install -y ./candidate.deb", "debian": "sudo apt-get install -y ./candidate.deb",
               "fedora": "sudo dnf -y install ./candidate.rpm", "arch": "sudo pacman -U --noconfirm ./candidate.pkg.tar.zst"}[marker["distro"]]
    script = "set -eu\ntest -f /etc/usbguard-clean-install-vm\n" + install + "\n"
    script += "usbguard-gui --help\nusbguard-gui --diagnose > baseline/diagnose.txt || test $? -eq 3\n"
    script += "sudo bash ./policy.sh\nflock --wait 300 ui-test.lock dbus-run-session -- python3 ./ui-test.py\n"
    with (directory / "validation.log").open("w") as log:
        command([*ssh_args(directory), "bash -c " + shlex.quote(script)], stdout=log, stderr=subprocess.STDOUT)
    result = subprocess.check_output([*ssh_args(directory), "cat ui-report.json"], text=True)
    report = {**marker, "package": package.name, "package_sha256": hashlib.sha256(package.read_bytes()).hexdigest(),
              "ui": json.loads(result)}
    (directory / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


def stop(directory):
    # QMP addresses this VM's private monitor; never terminate an arbitrary PID.
    with socket.socket(socket.AF_UNIX) as monitor:
        monitor.connect(str(directory / "qmp.sock"))
        stream = monitor.makefile("rwb", buffering=0)
        stream.readline()
        stream.write(b'{"execute":"qmp_capabilities"}\n')
        stream.readline()
        stream.write(b'{"execute":"quit"}\n')
    print("Disposable VM stopped")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="action", required=True)
    start = commands.add_parser("prepare")
    start.add_argument("--distro", choices=("ubuntu", "debian", "fedora", "arch"), required=True)
    start.add_argument("--image", type=Path, required=True)
    start.add_argument("--sha256", required=True)
    start.add_argument("--output", type=Path, required=True)
    run = commands.add_parser("ssh")
    run.add_argument("--output", type=Path, required=True)
    run.add_argument("command", nargs=argparse.REMAINDER)
    copy = commands.add_parser("copy")
    copy.add_argument("--output", type=Path, required=True)
    copy.add_argument("source", type=Path)
    copy.add_argument("destination", help="Absolute path in the guest user's home")
    test = commands.add_parser("validate")
    test.add_argument("--output", type=Path, required=True)
    test.add_argument("--package", type=Path, required=True)
    collect = commands.add_parser("collect")
    collect.add_argument("--output", type=Path, required=True)
    finish = commands.add_parser("stop")
    finish.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.action == "prepare":
        prepare(args)
    elif args.action == "ssh":
        if not args.command:
            parser.error("A guest command is required")
        command([*ssh_args(args.output.resolve()), shlex.join(args.command)])
    elif args.action == "collect":
        with (args.output / "guest-evidence.tar.gz").open("wb") as evidence:
            command([*ssh_args(args.output.resolve()), "tar --ignore-failed-read -czf - baseline ui.log ui-report.json"], stdout=evidence)
    elif args.action == "validate":
        validate(args)
    elif args.action == "copy":
        if not args.destination.startswith(f"/home/{USER}/") or ".." in Path(args.destination).parts:
            parser.error("Destination must be inside the guest test user's home")
        with args.source.open("rb") as source:
            command([*ssh_args(args.output.resolve()),
                     "umask 077; cat > " + shlex.quote(args.destination)], stdin=source)
    else:
        stop(args.output.resolve())


if __name__ == "__main__":
    main()
