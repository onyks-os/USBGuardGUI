#!/usr/bin/env python3
"""Compare two clean, offline GUI binary and native/Flatpak package builds in the pinned Linux/amd64 builder."""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RECIPE = ROOT / "packaging/reproducible"
SOURCE_PATHS = (
    "Cargo.toml", "Cargo.lock", "build.rs", "src", "data", "po", "assets",
    "README.md", "LICENSE", ".cargo/config", ".cargo/config.toml", "rust-toolchain.toml",
    "packaging/70-usbguard-gui.rules.example", "packaging/arch/PKGBUILD", "packaging/io.github.onyks_os.UsbguardGui.yaml", "tests/fixtures",
)


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def source_files(root):
    names = git(root, "ls-files", "--cached", "--others", "--exclude-standard",
                "-z", "--", *SOURCE_PATHS).split(b"\0")
    files = sorted({Path(os.fsdecode(name)) for name in names if name})
    return [name for name in files if (root / name).exists() or (root / name).is_symlink()]


def source_archive(root, files, epoch, destination):
    """Normalize file ordering, ownership, modes, and mtimes without touching source files."""
    with tarfile.open(destination, "w", format=tarfile.USTAR_FORMAT) as archive:
        for name in sorted(files):
            path = root / name
            if name.is_absolute() or ".." in name.parts or path.is_symlink() or not path.is_file():
                raise ValueError(f"Source must be a regular file inside the checkout: {name}")
            info = tarfile.TarInfo(name.as_posix())
            info.size = path.stat().st_size
            info.mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
            info.mtime = epoch
            with path.open("rb") as content:
                archive.addfile(info, content)
    return sha256(destination)


def sha256(path):
    with path.open("rb") as content:
        return hashlib.file_digest(content, "sha256").hexdigest()


def recipe_digest(recipe, driver):
    digest = hashlib.sha256()
    for path in [*sorted(recipe.iterdir()), driver]:
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"Unexpected recipe input: {path}")
        digest.update(path.name.encode() + b"\0" + path.read_bytes() + b"\0")
    return digest.hexdigest()


def verify_binaries(first, second):
    if not first.is_file() or not second.is_file() or first.stat().st_size == 0:
        raise ValueError("Both builds must produce a non-empty binary")
    data = first.read_bytes()
    if data != second.read_bytes():
        raise ValueError(f"Binaries differ: {sha256(first)} != {sha256(second)}")
    for path in (b"/tmp/repro-one", b"/tmp/independent/repro-two", b"/dependencies/"):
        if path in data:
            raise ValueError(f"Unnormalized build path in binary: {path.decode()}")
    return sha256(first)


def verify_packages(first, second, extension="rpm"):
    """Compare the complete package, including headers and compressed payload."""
    label = extension.upper()
    packages = [sorted(directory.glob(f"*.{extension}")) for directory in (first, second)]
    if any(len(items) != 1 for items in packages):
        raise ValueError(f"Each build must produce exactly one {label}")
    left, right = packages[0][0], packages[1][0]
    if left.name != right.name or not left.stat().st_size or left.read_bytes() != right.read_bytes():
        raise ValueError(f"{label} packages differ or are empty")
    return left.name, sha256(left)


def logged(command, log):
    print(f"Running {command[0]} {command[1]} (log: {log})", flush=True)
    with log.open("w") as output:
        result = subprocess.run(command, stdout=output, stderr=subprocess.STDOUT, check=False)
    if result.returncode:
        print("\n".join(log.read_text(errors="replace").splitlines()[-40:]), file=sys.stderr)
        raise RuntimeError(f"Command failed ({result.returncode}); see {log}")


def container_command(engine, image, inputs, output, epoch, phase, vendor=None):
    command = [engine, "run", "--rm", "--platform=linux/amd64"]
    if Path(engine).name == "podman":
        command.append("--userns=keep-id")
    command += ["--user", f"{os.getuid()}:{os.getgid()}", "--pull=never",
                "--env", f"SOURCE_DATE_EPOCH={epoch}",
                "--volume", f"{inputs}:/input:ro,Z", "--volume", f"{output}:/output:Z"]
    if vendor is not None:
        command += ["--network=none", "--volume", f"{vendor}:/dependencies:ro,Z"]
    return [*command, image, phase]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worktree", action="store_true",
                        help="explicitly include local modifications in the source snapshot")
    parser.add_argument("--output", type=Path, default=ROOT / "target/reproducible",
                        help="new output directory (must not already exist)")
    parser.add_argument("--engine", default=os.environ.get("CONTAINER_ENGINE", "podman"),
                        help="podman or docker executable")
    args = parser.parse_args(argv)
    if shutil.which(args.engine) is None:
        parser.error(f"Container engine not found: {args.engine}")
    dirty = bool(git(ROOT, "status", "--porcelain", "--untracked-files=all").strip())
    if dirty and not args.worktree:
        parser.error("Checkout is modified; commit first or explicitly use --worktree")
    output = args.output.resolve()
    if output.exists():
        parser.error(f"Output directory already exists; choose a fresh --output: {output}")
    files = source_files(ROOT)
    for required in ("Cargo.toml", "Cargo.lock", "build.rs", "src/main.rs", "src/lib.rs",
                     "packaging/70-usbguard-gui.rules.example"):
        if Path(required) not in files:
            parser.error(f"Missing source input: {required}")
    revision = git(ROOT, "rev-parse", "HEAD").decode().strip()
    epoch = int(git(ROOT, "show", "-s", "--format=%ct", "HEAD").decode())
    output.mkdir(parents=True)
    inputs = output / "inputs"
    inputs.mkdir()
    source_hash = source_archive(ROOT, files, epoch, inputs / "source.tar")
    # Freeze the recipe too, so edits during a long build cannot change what
    # the recorded recipe digest means. Preserve it with the measured input.
    recipe = inputs / "recipe"
    driver = inputs / "reproducible-build.py"
    shutil.copytree(RECIPE, recipe, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    shutil.copyfile(Path(__file__).resolve(), driver)
    recipe_hash = recipe_digest(recipe, driver)
    recipe_archive_hash = source_archive(
        inputs, [path.relative_to(inputs) for path in recipe.iterdir()] + [driver.relative_to(inputs)],
        epoch, inputs / "recipe.tar")
    image = f"localhost/usbguard-gui-reproducible:{recipe_hash[:16]}"
    print(f"Source snapshot: {source_hash}; commit {revision}; modified={dirty}", flush=True)
    logged([args.engine, "build", "--platform=linux/amd64", "--file", str(recipe / "Containerfile"),
            "--tag", image, str(recipe)], output / "builder.log")
    image_id = subprocess.check_output(
        [args.engine, "image", "inspect", image, "--format", "{{.Id}}"]
    ).decode().strip()
    # Only source input and final results survive. There is no compiler/target
    # cache shared between the two independent, network-disabled containers.
    with tempfile.TemporaryDirectory(prefix="prepare-", dir=output) as temporary:
        preparation = Path(temporary)
        logged(container_command(args.engine, image, inputs, preparation, epoch, "prepare"),
               output / "prepare.log")
        for name in ("packages.txt", "toolchain.txt"):
            shutil.copyfile(preparation / name, output / name)
        for phase in ("first", "second"):
            result = output / phase
            result.mkdir()
            logged(container_command(args.engine, image, inputs, result, epoch, phase,
                                     preparation / "vendor"), output / f"{phase}.log")
    digest = verify_binaries(output / "first/usbguard-gui", output / "second/usbguard-gui")
    for phase in ("first", "second"):
        dynamic = (output / phase / "elf-dynamic.txt").read_text()
        if "libgtk-4.so.1" not in dynamic or "libadwaita-1.so.0" not in dynamic:
            raise ValueError("The measured artifact must include the GUI")
    rpm_name, rpm_digest = verify_packages(output / "first", output / "second")
    deb_name, deb_digest = verify_packages(output / "first", output / "second", "deb")
    arch_name, arch_digest = verify_packages(output / "first", output / "second", "pkg.tar.zst")
    flatpak_name, flatpak_digest = verify_packages(output / "first", output / "second", "flatpak")
    flatpak_binary_digest = verify_binaries(output / "first/flatpak-bin/usbguard-gui",
                                            output / "second/flatpak-bin/usbguard-gui")
    report = {
        "scope": "Linux x86_64 GUI binary, unsigned RPM, DEB, Arch package and Flatpak bundle",
        "revision": revision, "modified_worktree": dirty,
        "source_date_epoch": epoch, "source_sha256": source_hash,
        "recipe_sha256": recipe_hash, "recipe_archive_sha256": recipe_archive_hash,
        "builder_image": image_id,
        "flatpak_filename": flatpak_name, "flatpak_sha256": flatpak_digest,
        "flatpak_binary_sha256": flatpak_binary_digest,
        "arch_filename": arch_name, "arch_sha256": arch_digest,
        "deb_filename": deb_name, "deb_sha256": deb_digest,
        "rpm_filename": rpm_name, "rpm_sha256": rpm_digest,
        "binary_sha256": digest, "byte_identical": True,
        "build_roots": ["/tmp/repro-one", "/tmp/independent/repro-two"],
        "compilation_network": "none",
    }
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    (output / "SHA256SUMS").write_text(
        f"{digest}  first/usbguard-gui\n{digest}  second/usbguard-gui\n"
        f"{rpm_digest}  first/{rpm_name}\n{rpm_digest}  second/{rpm_name}\n"
        f"{deb_digest}  first/{deb_name}\n{deb_digest}  second/{deb_name}\n"
        f"{arch_digest}  first/{arch_name}\n{arch_digest}  second/{arch_name}\n"
        f"{flatpak_digest}  first/{flatpak_name}\n{flatpak_digest}  second/{flatpak_name}\n"
        f"{flatpak_binary_digest}  first/flatpak-bin/usbguard-gui\n"
        f"{flatpak_binary_digest}  second/flatpak-bin/usbguard-gui\n"
    )
    print(f"PASS: byte-identical GUI binaries, RPMs, DEBs, Arch packages and Flatpak bundles, SHA-256 {digest}\nReport: {output / 'report.json'}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"Reproducibility check failed: {error}", file=sys.stderr)
        sys.exit(1)
