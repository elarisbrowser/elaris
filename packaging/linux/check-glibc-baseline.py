#!/usr/bin/env python3
"""Fail when an ELF file requires a glibc symbol newer than the release baseline."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

VERSION = re.compile(r"\bGLIBC_(\d+(?:\.\d+)+)\b")


def version(value: str) -> tuple[int, ...]:
    return tuple(int(part) for part in value.split("."))


def main() -> int:
    if len(sys.argv) != 3:
        print(f"usage: {Path(sys.argv[0]).name} <bundle-root> <glibc-version>", file=sys.stderr)
        return 2

    root = Path(sys.argv[1])
    baseline = version(sys.argv[2])
    failures: list[tuple[Path, str]] = []
    for path in sorted(item for item in root.rglob("*") if item.is_file()):
        header = subprocess.run(["readelf", "-h", str(path)], capture_output=True)
        if header.returncode:
            continue
        info = subprocess.run(["readelf", "--version-info", str(path)], capture_output=True, text=True)
        if info.returncode:
            print(f"Could not inspect ELF symbol versions in {path}", file=sys.stderr)
            return 2
        required = max((version(match) for match in VERSION.findall(info.stdout)), default=())
        if required > baseline:
            failures.append((path, ".".join(map(str, required))))

    if failures:
        for path, required in failures:
            print(f"{path}: requires GLIBC_{required}, baseline is GLIBC_{sys.argv[2]}", file=sys.stderr)
        return 1
    print(f"All bundled ELF files fit the GLIBC_{sys.argv[2]} baseline")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
