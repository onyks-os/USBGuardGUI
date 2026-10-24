#!/usr/bin/env python3
"""Install a bundle in a private Flatpak installation and test its pinned runtime."""

import argparse
import hashlib
import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
APP = "io.github.onyks_os.UsbguardGui"


def verify(bundle, output, lockfile, runtime_repository=None, runtime_bundle=None):
    bundle, output = bundle.resolve(), output.resolve()
    if not bundle.is_file():
        raise FileNotFoundError(bundle)
    lock = json.loads(lockfile.read_text())
    output.mkdir(parents=True, exist_ok=False)
    env = {**os.environ, "HOME": str(output / "home"),
           "FLATPAK_USER_DIR": str(output / "installation"),
           "XDG_DATA_HOME": str(output / "data"), "XDG_CACHE_HOME": str(output / "cache"),
           "XDG_CONFIG_HOME": str(output / "config")}
    for key in ("HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_CONFIG_HOME"):
        Path(env[key]).mkdir()

    def run(*args):
        result = subprocess.run(["flatpak", *args], env=env, check=False, capture_output=True, text=True)
        with (output / "commands.log").open("a") as log:
            log.write("flatpak " + " ".join(map(str, args)) + "\n" + result.stdout + result.stderr)
        result.check_returncode()
        return result.stdout.strip()

    if runtime_bundle is not None:
        run("install", "--user", "--assumeyes", "--no-related", "--no-deps", "--bundle", str(runtime_bundle.resolve()))
    elif runtime_repository is None:
        run("remote-add", "--user", "--from", "repro-runtime", lock["repository_file"])
        run("install", "--user", "--assumeyes", "--no-related", "--no-deps", "repro-runtime", lock["ref"])
        run("update", "--user", "--assumeyes", "--no-related", "--no-deps", "--commit=" + lock["commit"], lock["ref"])
    else:
        # Installed OSTree repositories have no remote summary. Export the
        # runtime read-only and import it into our private installation instead.
        _, runtime_id, architecture, branch = lock["ref"].split("/")
        runtime_bundle = output / "runtime.flatpak"
        run("build-bundle", "--runtime", "--arch=" + architecture,
            str(runtime_repository.resolve()), str(runtime_bundle), runtime_id, branch)
        run("install", "--user", "--assumeyes", "--no-related", "--no-deps", "--bundle", str(runtime_bundle))
    actual = run("info", "--user", "--show-commit", lock["ref"])
    if actual != lock["commit"]:
        raise ValueError("Installed Flatpak runtime does not match the pinned commit")
    run("install", "--user", "--assumeyes", "--no-related", "--no-deps", "--bundle", str(bundle))
    help_text = run("run", "--user", APP, "--help")
    if "Usage: usbguard-gui" not in help_text:
        raise ValueError("Installed Flatpak CLI did not start correctly")
    # Keep this aligned with the catalogue list used by all package builders.
    languages = [line.strip() for line in (ROOT / "po/LINGUAS").read_text().splitlines()
                 if line.strip() and not line.lstrip().startswith("#")]
    if not languages or any(not all(c.isalnum() or c in "_@-" for c in lang) for lang in languages):
        raise ValueError("Invalid translation language list")
    checks = 'set -eu\n' + ''.join(
        f'test -f /app/share/locale/{lang}/LC_MESSAGES/usbguard-gui.mo\n' for lang in languages)
    checks += '''
    test -f /app/share/doc/usbguard-gui/70-usbguard-gui.rules.example
    gsettings list-schemas | grep -Fx io.github.onyks_os.UsbguardGui
    glib-compile-schemas --strict --dry-run /app/share/glib-2.0/schemas
    '''
    run("run", "--user", "--command=sh", APP, "-c", checks)
    report = {"runtime_ref": lock["ref"], "runtime_commit": actual,
              "bundle_sha256": hashlib.sha256(bundle.read_bytes()).hexdigest(),
              "installed_cli": True, "installed_schema": True, "installed_assets": True,
              "installed_languages": languages}
    (output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    return report


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--runtime-lock", type=Path, default=ROOT / "packaging/reproducible/flatpak-runtime.json")
    runtime_source = parser.add_mutually_exclusive_group()
    runtime_source.add_argument("--runtime-bundle", type=Path, help="reuse a previously exported runtime bundle")
    runtime_source.add_argument("--runtime-repository", type=Path,
                        help="reuse a trusted local runtime OSTree repository, read-only")
    args = parser.parse_args(argv)
    verify(args.bundle, args.output, args.runtime_lock, args.runtime_repository, args.runtime_bundle)


if __name__ == "__main__":
    main()
