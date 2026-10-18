#!/usr/bin/env python3
"""Build the DEB with pinned dpkg tools and the verified ELF, without scripts."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tomllib


def stage(source, binary, destination):
    package = tomllib.loads((source / "Cargo.toml").read_text())["package"]
    metadata = package["metadata"]["deb"]
    checksums = []
    assets = [*metadata["assets"],
              ["LICENSE", f"usr/share/doc/{package['name']}/copyright", "644"]]
    for original, name, mode in assets:
        relative = Path(name)
        if relative.is_absolute() or ".." in relative.parts or relative.parts[0] != "usr":
            raise ValueError(f"Invalid DEB destination: {name}")
        if name.endswith("/"):
            relative /= Path(original).name
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(binary if original == "target/release/usbguard-gui" else source / original, target)
        target.chmod(int(mode, 8))
        checksums.append(f"{hashlib.md5(target.read_bytes(), usedforsecurity=False).hexdigest()}  {relative.as_posix()}\n")
    control = destination / "DEBIAN"
    control.mkdir()
    (control / "md5sums").write_text("".join(sorted(checksums)))
    return package


def control_text(package, automatic):
    metadata = package["metadata"]["deb"]
    if not automatic.startswith("shlibs:Depends=") or not automatic.removeprefix("shlibs:Depends=").strip():
        raise ValueError("Missing automatic shared-library dependencies")
    depends = metadata["depends"].replace("$auto", automatic.removeprefix("shlibs:Depends=").strip())
    description = "\n".join(" " + (line or ".") for line in metadata["extended-description"].splitlines())
    return (f"Package: {package['name']}\nVersion: {package['version']}-1\nArchitecture: amd64\n"
            f"Maintainer: {metadata['maintainer']}\nSection: {metadata['section']}\n"
            f"Priority: {metadata['priority']}\nHomepage: {package['repository']}\n"
            f"Depends: {depends}\nRecommends: {metadata['recommends']}\n"
            f"Description: {package['description']}\n{description}\n")


def main(output=Path("/output")):
    source = Path.cwd()
    work = source.parent / "deb"
    payload = work / "payload"
    package = stage(source, output / "usbguard-gui", payload)
    debian = work / "debian"
    debian.mkdir()
    (debian / "control").write_text(
        f"Source: {package['name']}\n\nPackage: {package['name']}\nArchitecture: amd64\n")
    automatic = subprocess.check_output(
        ["dpkg-shlibdeps", "-O", "-e" + str(output / "usbguard-gui")], cwd=work, text=True).strip()
    (payload / "DEBIAN/control").write_text(control_text(package, automatic))
    epoch = int(os.environ["SOURCE_DATE_EPOCH"])
    for path in [payload, *payload.rglob("*")]:
        if path.is_dir():
            path.chmod(0o755)
        elif path.parent.name == "DEBIAN":
            path.chmod(0o644)
        os.utime(path, (epoch, epoch))
    artifact = output / f"{package['name']}_{package['version']}-1_amd64.deb"
    subprocess.run(["dpkg-deb", "--root-owner-group", "--threads-max=1", "-Zxz", "-z9",
                    "--build", str(payload), str(artifact)], check=True)
    for name, options in [("deb-files.txt", ["--contents"]), ("deb-control.txt", ["--info"])]:
        with (output / name).open("w") as query_output:
            subprocess.run(["dpkg-deb", *options, str(artifact)], stdout=query_output, check=True)


if __name__ == "__main__":
    main()
