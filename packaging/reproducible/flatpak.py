#!/usr/bin/env python3
"""Stage the Flatpak manifest's assets and export a deterministic OSTree bundle."""

import os
import shlex
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path

import tomllib
import yaml


def manifest(source):
    data = yaml.safe_load((source / "packaging/io.github.onyks_os.UsbguardGui.yaml").read_text())
    if data["app-id"] != "io.github.onyks_os.UsbguardGui" or len(data["modules"]) != 1:
        raise ValueError("Unexpected Flatpak application/modules")
    commands = data["modules"][0]["build-commands"]
    if not commands[0].startswith("cargo ") or not commands[1].startswith("cargo "):
        raise ValueError("Expected the manifest's Cargo fetch/build steps")
    return data, commands[2:]


def main(output=Path("/output")):
    source = Path.cwd()
    data, commands = manifest(source)
    application = data["app-id"]
    work = source.parent / "flatpak"
    build = work / "build"
    files = build / "files"
    files.mkdir(parents=True)
    metadata = (f"[Application]\nname={application}\n"
                f"runtime={data['runtime']}/x86_64/{data['runtime-version']}\n"
                f"sdk={data['sdk']}/x86_64/{data['runtime-version']}\ncommand={data['command']}\n")
    (build / "metadata").write_text(metadata)
    binary = output / "flatpak-bin/usbguard-gui"
    for command in commands:
        command = command.replace("target/release/usbguard-gui", shlex.quote(str(binary)))
        command = command.replace("/app/", shlex.quote(str(files)) + "/")
        subprocess.run(["bash", "-euc", command], check=True)
    if (files / "bin/usbguard-gui").read_bytes() != binary.read_bytes():
        raise ValueError("Flatpak payload differs from its verified binary")
    subprocess.run(["flatpak", "build-finish", *data["finish-args"], str(build)], check=True)
    epoch = int(os.environ["SOURCE_DATE_EPOCH"])
    for path in [build, *build.rglob("*")]:
        if not path.is_symlink():
            os.utime(path, (epoch, epoch))
    repository = work / "repo"
    # The validator only sees our staged icon files. Container seccomp blocks
    # nested sandboxing; the entire export already runs in the offline container.
    subprocess.run(["flatpak", "build-export", "--disable-sandbox", "--timestamp=" + datetime.fromtimestamp(epoch, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                    str(repository), str(build), "stable"], check=True)
    package = tomllib.loads((source / "Cargo.toml").read_text())["package"]
    artifact = output / f"usbguard-gui-{package['version']}.flatpak"
    # libostree also embeds a separate delta-generation wall-clock timestamp.
    # Freeze only this exporter process; do not modify or ignore bundle bytes.
    clock = {**os.environ,
             "LD_PRELOAD": "/usr/lib/x86_64-linux-gnu/faketime/libfaketime.so.1",
             "FAKETIME": datetime.fromtimestamp(epoch, timezone.utc).strftime("%Y-%m-%d %H:%M:%S"),
             "FAKETIME_DONT_FAKE_MONOTONIC": "1", "TZ": "UTC"}
    subprocess.run(["flatpak", "build-bundle", "--runtime-repo=https://flathub.org/repo/flathub.flatpakrepo",
                    str(repository), str(artifact), application, "stable"], env=clock, check=True)
    imported = work / "imported"
    subprocess.run(["ostree", f"--repo={imported}", "init", "--mode=archive"], check=True)
    subprocess.run(["flatpak", "build-import-bundle", str(imported), str(artifact)], check=True)
    ref = f"app/{application}/x86_64/stable"
    checkout = output / "flatpak-checkout"
    subprocess.run(["ostree", f"--repo={imported}", "checkout", "--user-mode", ref, str(checkout)], check=True)
    if (checkout / "files/bin/usbguard-gui").read_bytes() != binary.read_bytes():
        raise ValueError("Imported Flatpak binary differs")
    shutil.copyfile(checkout / "metadata", output / "flatpak-metadata.txt")
    with (output / "flatpak-commit.txt").open("w") as log:
        subprocess.run(["ostree", f"--repo={imported}", "rev-parse", ref], stdout=log, check=True)


if __name__ == "__main__":
    main()
