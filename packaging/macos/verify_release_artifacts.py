#!/usr/bin/env python3
"""Fail closed unless the release directory contains one verified notarized macOS artifact."""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from pathlib import Path


class ArtifactError(RuntimeError):
    """A release directory is incomplete, unsigned, or internally inconsistent."""


def verify_release_artifacts(directory: Path) -> Path:
    """Require a notarization marker and a checksum that matches the one release DMG."""
    packages = sorted(directory.glob("Elaris-*-macos-arm64.dmg"))
    if len(packages) != 1:
        raise ArtifactError(f"Expected exactly one signed macOS DMG in {directory}, found {len(packages)}.")
    dmg = packages[0]
    required_files = (
        dmg.with_name(dmg.name + ".sha256"),
        directory / "SIGNING-INFO.txt",
        directory / f"{dmg.stem}.BUILD-INFO.txt",
    )
    for path in required_files:
        if not path.is_file() or path.stat().st_size == 0:
            raise ArtifactError(f"Missing or empty release metadata: {path.name}")

    signing_info = (directory / "SIGNING-INFO.txt").read_text()
    expected = {
        "Status": "notarized",
        "Identity type": "Developer ID Application",
        "Hardened Runtime": "enabled for all Mach-O code",
        "Notarization": "accepted",
        "Stapling": "validated",
    }
    fields = dict(
        line.split(": ", 1)
        for line in signing_info.splitlines()
        if ": " in line
    )
    for key, value in expected.items():
        if fields.get(key) != value:
            raise ArtifactError(f"Signing metadata does not confirm {key.lower()}.")
    team_id = fields.get("Team ID", "")
    if not re.fullmatch(r"[A-Z0-9]{10}", team_id):
        raise ArtifactError("Signing metadata has no valid Apple Developer team ID.")
    if not fields.get("Submission ID"):
        raise ArtifactError("Signing metadata has no Apple notarization submission ID.")

    checksum_line = required_files[0].read_text().strip()
    checksum, separator, filename = checksum_line.partition("  ")
    if separator != "  " or filename != dmg.name or not re.fullmatch(r"[0-9a-f]{64}", checksum):
        raise ArtifactError("The macOS DMG checksum file has an unexpected format.")
    digest = hashlib.sha256()
    with dmg.open("rb") as artifact:
        for block in iter(lambda: artifact.read(1024 * 1024), b""):
            digest.update(block)
    if digest.hexdigest() != checksum:
        raise ArtifactError("The macOS DMG checksum does not match its contents.")
    return dmg


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    try:
        artifact = verify_release_artifacts(args.directory)
    except (ArtifactError, OSError) as error:
        print(f"macOS release artifact validation failed: {error}", file=sys.stderr)
        return 1
    print(f"Verified signed and notarized macOS release artifact: {artifact.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
