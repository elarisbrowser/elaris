#!/usr/bin/env python3
"""Write THIRD_PARTY_NOTICES.md from the locked Rust dependencies, and flag licences that may need review.

Run from the repository root: `python3 tools/notices.py`. Use `--check` to fail when the file is out of date, so CI
can keep the notices in step with Cargo.lock.

The C++ section lists what the built app bundles: Ladybird, and the vcpkg packages that supply the libraries in each
bundle. Build the app bundles first (`./elaris bundle`), so the libraries exist to be listed. Library files that no vcpkg
package installed are flagged for review.
"""

from __future__ import annotations

import argparse
import json
import platform
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "THIRD_PARTY_NOTICES.md"

# A licence expression that names one of these without a permissive alternative needs a human decision.
COPYLEFT = re.compile(r"\b(GPL|LGPL|AGPL|MPL|EPL|CDDL)")
PERMISSIVE = re.compile(r"\b(MIT|Apache-2\.0|BSD-[23]-Clause|ISC|Zlib|Unicode-3\.0|BSL-1\.0|Unlicense|0BSD|CC0-1\.0)\b")


def cargo_packages() -> list[dict]:
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--format-version", "1"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    packages = json.loads(result.stdout)["packages"]
    # The workspace's own crate is not a third-party dependency.
    return sorted(
        (p for p in packages if p.get("source") is not None),
        key=lambda p: (p["name"].lower(), p["version"]),
    )


def needs_review(expression: str) -> bool:
    return bool(COPYLEFT.search(expression)) and not PERMISSIVE.search(expression)


VCPKG_ROOT = ROOT / "vendor" / "ladybird" / "Build" / "release" / "vcpkg_installed"
# Each shipped platform: its vcpkg triplet, its app bundle's library directory, and where its vcpkg metadata is read from.
# The Linux inputs are staged under build/notices-inputs (see docs/release.md) because the Linux bundle is built in the
# container; the macOS inputs come straight from the host's vcpkg installation.
PLATFORMS = {
    "arm64-osx-dynamic": {
        "libs": ROOT / "build" / "Elaris.app" / "Contents" / "lib",
        "info": VCPKG_ROOT / "vcpkg" / "info",
        "share": VCPKG_ROOT / "arm64-osx-dynamic" / "share",
    },
    "x64-linux-dynamic": {
        "libs": ROOT / "build" / "notices-inputs" / "x64-linux-dynamic" / "libs.txt",
        "info": ROOT / "build" / "notices-inputs" / "x64-linux-dynamic" / "info",
        "share": ROOT / "build" / "notices-inputs" / "x64-linux-dynamic" / "share",
    },
}
# Ladybird's own libraries are BSD-2-Clause; Elaris's bridge is part of this project.
LADYBIRD_PREFIXES = ("liblagom-",)
ELARIS_LIBRARIES = ("libelaris_ladybird",)


def bundled_library_names(triplet: str) -> set[str]:
    source = PLATFORMS[triplet]["libs"]
    if source.is_file():
        return {line.strip() for line in source.read_text().splitlines() if line.strip()}
    if source.is_dir():
        return {path.name for path in source.iterdir()}
    return set()


def vcpkg_packages(triplet: str) -> dict[str, tuple[str, str]]:
    """Maps each library file name vcpkg installed for `triplet` to (package, version)."""
    owners: dict[str, tuple[str, str]] = {}
    suffix = f"_{triplet}.list"
    info = PLATFORMS[triplet]["info"]
    for listing in sorted(info.glob(f"*{suffix}")) if info.is_dir() else []:
        stem = listing.name[: -len(suffix)]
        package, _, version = stem.partition("_")
        for line in listing.read_text().splitlines():
            if line.endswith("/") or "/lib/" not in line:
                continue
            owners.setdefault(line.rsplit("/", 1)[-1], (package, version or stem))
    return owners


def copyright_text(triplet: str, package: str) -> str | None:
    path = PLATFORMS[triplet]["share"] / package / "copyright"
    return path.read_text(errors="replace") if path.is_file() else None


def cpp_section() -> list[str]:
    lines = ["", "## C++ components", ""]
    triplets = [triplet for triplet in PLATFORMS if bundled_library_names(triplet)]
    if not triplets:
        return lines + ["No app bundle is built yet, so the bundled C++ components cannot be listed. Run `./elaris bundle`.", ""]

    found: dict[str, tuple[str, str]] = {}  # component label -> (package or "", triplet)
    unmatched: list[str] = []
    for triplet in triplets:
        owners = vcpkg_packages(triplet)
        for name in sorted(bundled_library_names(triplet)):
            if name.startswith(ELARIS_LIBRARIES):
                continue
            if name.startswith(LADYBIRD_PREFIXES):
                found.setdefault("Ladybird", ("", triplet))
            elif name in owners:
                package, version = owners[name]
                found.setdefault(f"{package} {version}", (package, triplet))
            else:
                unmatched.append(f"{name} ({triplet})")

    lines += [
        "Ladybird, the engine Elaris embeds, is BSD-2-Clause. Its licence text is in the appendix.",
        "",
        "Platforms covered: " + ", ".join(triplets) + ".",
        "",
        "| Component | Licence source |",
        "| --- | --- |",
    ]
    for label, (package, _) in sorted(found.items()):
        source = f"vcpkg `{package}`" if package else "Ladybird `LICENSE`"
        lines.append(f"| {label} | {source} |")
    if unmatched:
        lines += ["", "### Needs review", "", "These bundled libraries were not installed by vcpkg:", ""]
        lines += [f"- {name}" for name in unmatched]

    lines += ["", "### Licence texts", ""]
    ladybird = ROOT / "vendor" / "ladybird" / "LICENSE"
    if ladybird.is_file():
        lines += ["#### Ladybird", "", "```text", ladybird.read_text(errors="replace").rstrip(), "```", ""]
    for package, triplet in sorted({(package, triplet) for package, triplet in found.values() if package}):
        text = copyright_text(triplet, package)
        lines.append(f"#### {package}")
        lines.append("")
        if text is None:
            lines += ["No `copyright` file was found for this package; check its vcpkg port before release.", ""]
        else:
            lines += ["```text", text.rstrip(), "```", ""]
    return lines


def render(packages: list[dict]) -> str:
    counts: dict[str, int] = {}
    for package in packages:
        expression = package.get("license") or "UNKNOWN"
        counts[expression] = counts.get(expression, 0) + 1

    lines = [
        "# Third-party notices",
        "",
        "Elaris is licensed under GPL-3.0-only (see [LICENSE](LICENSE)). It includes the components below. This file is",
        "generated by `tools/notices.py` from `Cargo.lock`; regenerate it after changing dependencies.",
        "",
        "## Rust crates",
        "",
        f"{len(packages)} crates. Each entry gives the licence expression Cargo declares for that crate.",
        "",
        "| Crate | Version | Licence | Source |",
        "| --- | --- | --- | --- |",
    ]
    for package in packages:
        expression = (package.get("license") or "UNKNOWN").replace("|", "\\|")
        source = package.get("repository") or package.get("source") or ""
        lines.append(f"| {package['name']} | {package['version']} | {expression} | {source} |")

    flagged = sorted({p["name"] for p in packages if needs_review(p.get("license") or "")})
    lines += ["", "### Licence summary", "", "| Licence expression | Crates |", "| --- | --- |"]
    for expression, count in sorted(counts.items(), key=lambda item: (-item[1], item[0])):
        lines.append(f"| {expression.replace('|', chr(92) + '|')} | {count} |")
    if flagged:
        lines += ["", "### Needs review", "", "These crates declare only copyleft licences:", ""]
        lines += [f"- {name}" for name in flagged]

    lines += cpp_section()
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="fail if THIRD_PARTY_NOTICES.md is out of date")
    args = parser.parse_args()

    packages = cargo_packages()
    text = render(packages)
    flagged = [p["name"] for p in packages if needs_review(p.get("license") or "")]
    if args.check:
        current = OUTPUT.read_text() if OUTPUT.exists() else ""
        if current != text:
            print("THIRD_PARTY_NOTICES.md is out of date; run python3 tools/notices.py", file=sys.stderr)
            return 1
    else:
        OUTPUT.write_text(text)
        print(f"wrote {OUTPUT.relative_to(ROOT)} for {len(packages)} crates")
    if flagged:
        print("crates that declare only copyleft licences:", ", ".join(flagged), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
