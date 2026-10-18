#!/usr/bin/env python3
"""Stage the declared RPM assets and build without rewriting the verified ELF."""

import os
from pathlib import Path
import shutil
import subprocess
import tomllib


def stage(source, binary, destination, epoch):
    package = tomllib.loads((source / "Cargo.toml").read_text())["package"]
    metadata = package["metadata"]["generate-rpm"]
    files = []
    for asset in metadata["assets"]:
        relative = Path(asset["dest"].lstrip("/"))
        if ".." in relative.parts or not asset["dest"].startswith("/usr/"):
            raise ValueError(f"Invalid RPM destination: {asset['dest']}")
        original = binary if asset["source"] == "target/release/usbguard-gui" else source / asset["source"]
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(original, target)
        target.chmod(int(asset["mode"], 8))
        os.utime(target, (epoch, epoch))
        files.append(("%doc " if asset.get("doc") else "") + asset["dest"])
    lines = [f"Name: {package['name']}", f"Version: {package['version']}", "Release: 1",
             f"Summary: {metadata['summary']}", f"License: {package['license']}",
             f"URL: {package['repository']}", "BuildArch: x86_64"]
    for key, tag in [("requires", "Requires"), ("recommends", "Recommends")]:
        for name, constraint in sorted(metadata.get(key, {}).items()):
            lines.append(f"{tag}: {name}" + (f" {constraint}" if constraint != "*" else ""))
    # RPM's automatic ELF dependencies remain enabled. Disable only rewriting
    # (strip/debug/build-id links): the payload must retain the verified binary.
    lines += ["%global __os_install_post %{nil}", "%global debug_package %{nil}",
              "%global _build_id_links none", "%description", package["description"],
              "%files", "%defattr(-,root,root,-)", *sorted(files), ""]
    return "\n".join(lines)


def main(output=Path("/output")):
    source = Path.cwd()
    epoch = int(os.environ["SOURCE_DATE_EPOCH"])
    top = source.parent / "rpm"
    buildroot = top / "payload"
    spec = stage(source, output / "usbguard-gui", buildroot, epoch)
    specfile = top / "usbguard-gui.spec"
    specfile.write_text(spec)
    command = ["rpmbuild", "-bb", "--buildroot", str(buildroot)]
    for definition in [f"_topdir {top}", "_buildhost reproducible.invalid",
                       "use_source_date_epoch_as_buildtime 1",
                       "clamp_mtime_to_source_date_epoch 1", "_binary_payload w9.gzdio"]:
        command += ["--define", definition]
    subprocess.run([*command, str(specfile)], check=True)
    packages = list((top / "RPMS").rglob("*.rpm"))
    if len(packages) != 1:
        raise ValueError("Expected exactly one RPM")
    rpm = output / packages[0].name
    shutil.copyfile(packages[0], rpm)
    subprocess.run(["rpm", "-K", str(rpm)], check=True)
    for name, options in [("rpm-files.txt", ["-qplv"]), ("rpm-requires.txt", ["-qpR"]),
                          ("rpm-scripts.txt", ["-qp", "--scripts"])]:
        with (output / name).open("w") as query_output:
            subprocess.run(["rpm", *options, str(rpm)], stdout=query_output, check=True)
    shutil.copyfile(specfile, output / "usbguard-gui.spec")


if __name__ == "__main__":
    main()
