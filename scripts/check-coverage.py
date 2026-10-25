#!/usr/bin/env python3
"""Enforce unrounded production line coverage, including every GTK module."""

import argparse
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# These contain only declarations/exports/constants/types, with no instrumentable code.
DECLARATIONS = {"src/lib.rs", "src/dbus/mod.rs", "src/model/mod.rs", "src/model/event.rs"}


def required_files(root: Path, core: bool) -> list[Path]:
    if core:
        return sorted((root / "src/rules").glob("*.rs")) + [
            root / "src/dbus/commands.rs", root / "src/dbus/errors.rs",
        ]
    return sorted(p for p in (root / "src").rglob("*.rs")
                  if p.relative_to(root).as_posix() not in DECLARATIONS)


def check(report: dict, root: Path, core: bool = False) -> int:
    files = {}
    for unit in report["data"]:
        for item in unit["files"]:
            path = Path(item["filename"]).resolve()
            if not path.is_relative_to(root / "src"):
                continue
            if path in files:
                raise ValueError(f"duplicate coverage: {path.relative_to(root)}")
            lines = item["summary"]["lines"]
            count, covered = lines["count"], lines["covered"]
            if (type(count) is not int or type(covered) is not int
                    or not 0 <= covered <= count):
                raise ValueError(f"invalid line counters: {path.relative_to(root)}")
            files[path] = (count, covered)
    failed = False
    for path in required_files(root, core):
        counters = files.get(path)
        if counters is None or counters[0] == 0:
            print(f"FAIL {path.relative_to(root)}: missing coverage")
            failed = True
        elif core:
            count, covered = counters
            passed = covered * 100 > count * 90
            print(f"{'PASS' if passed else 'FAIL'} {path.relative_to(root)}: "
                  f"{covered}/{count} ({100 * covered / count:.2f}%; required >90%)")
            failed |= not passed
    if not core:
        count = sum(c[0] for c in files.values())
        covered = sum(c[1] for c in files.values())
        passed = count > 0 and covered * 100 >= count * 98
        percent = 100 * covered / count if count else 0
        print(f"{'PASS' if passed else 'FAIL'} application (GUI included): "
              f"{covered}/{count} lines ({percent:.4f}%; required >=98%)")
        failed |= not passed
    return int(failed)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    parser.add_argument("--core", action="store_true", help="legacy >90%% per critical file")
    args = parser.parse_args()
    try:
        return check(json.loads(args.report.read_text()), ROOT, args.core)
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(f"FAIL invalid coverage report: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
