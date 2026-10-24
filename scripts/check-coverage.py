#!/usr/bin/env python3
"""Enforce line coverage for the rule language and cancellable-operation core."""

import json
import sys
from pathlib import Path


def main():
    if len(sys.argv) != 2:
        print("Usage: scripts/check-coverage.py <cargo-llvm-cov.json>", file=sys.stderr)
        return 2
    root = Path(__file__).resolve().parent.parent
    required = sorted((root / "src/rules").glob("*.rs")) + [
        root / "src/dbus/commands.rs",
        root / "src/dbus/errors.rs",
    ]
    report = json.loads(Path(sys.argv[1]).read_text())
    files = {
        Path(item["filename"]).resolve(): item["summary"]["lines"]
        for unit in report["data"]
        for item in unit["files"]
    }
    failed = False
    for path in required:
        relative = path.relative_to(root)
        lines = files.get(path)
        if lines is None or lines["count"] == 0:
            print(f"FAIL {relative}: missing coverage")
            failed = True
            continue
        # The architecture requires more than 90%, rather than a rounded 90%.
        passed = lines["covered"] * 100 > lines["count"] * 90
        print(f"{'PASS' if passed else 'FAIL'} {relative}: "
              f"{lines['covered']}/{lines['count']} lines "
              f"({100 * lines['covered'] / lines['count']:.2f}%; required >90%)")
        failed |= not passed
    return int(failed)


if __name__ == "__main__":
    sys.exit(main())
