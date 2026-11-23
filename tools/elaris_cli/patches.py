"""Check that Elaris's Ladybird patches still apply to a Ladybird revision."""

import argparse
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Optional

from . import ui
from .common import ENGINE, ROOT, require

PATCH_DIR = ROOT / "bridge" / "patches"
UPSTREAM_BRANCH = "master"
_TARGET = re.compile(r"^\+\+\+ b/(\S+)", re.MULTILINE)


@dataclass
class Result:
    name: str
    status: str  # "clean", "fuzzy" or "conflict"
    detail: str = ""


def targets(patch: Path) -> list:
    """Upstream files a patch changes."""
    return _TARGET.findall(patch.read_text())


def _source(path: str, ref: Optional[str]) -> Optional[bytes]:
    """A file's contents at `ref`, or in the checkout when `ref` is None; None if it does not exist."""
    if ref is None:
        file = ENGINE / path
        return file.read_bytes() if file.is_file() else None
    result = subprocess.run(("git", "show", f"{ref}:{path}"), cwd=ENGINE, capture_output=True, check=False)
    return result.stdout if result.returncode == 0 else None


def _stage(patch: Path, ref: Optional[str], staging: Path) -> Optional[str]:
    """Copy a patch's upstream files into `staging`; return the first missing path, if any."""
    for path in targets(patch):
        contents = _source(path, ref)
        if contents is None:
            return path
        staged = staging / path
        staged.parent.mkdir(parents=True, exist_ok=True)
        staged.write_bytes(contents)
    return None


def _apply(patch: Path, staging: Path, dry_run: bool) -> subprocess.CompletedProcess:
    command = ["patch", "--verbose", "--batch", "--forward", "--no-backup-if-mismatch", "-p1", "-i", str(patch)]
    if dry_run:
        command.insert(1, "--dry-run")
    return subprocess.run(command, cwd=staging, capture_output=True, text=True, check=False)


def check_patch(patch: Path, ref: Optional[str]) -> Result:
    with tempfile.TemporaryDirectory() as staging:
        missing = _stage(patch, ref, Path(staging))
        if missing:
            return Result(patch.stem, "conflict", f"{missing} no longer exists")
        result = _apply(patch, Path(staging), dry_run=True)
    output = result.stdout + result.stderr
    # Some `patch` builds exit 0 after skipping a reversed or already-applied section.
    failed = [line.strip() for line in output.splitlines() if re.search(r"failed|reversed|ignored", line, re.I)]
    if result.returncode or failed:
        return Result(patch.stem, "conflict", "; ".join(failed) or "does not apply")
    if "fuzz" in output or "offset" in output:
        return Result(patch.stem, "fuzzy", "applies with offset or fuzz; refresh it")
    return Result(patch.stem, "clean")


def refresh_patch(patch: Path) -> None:
    """Regenerate a patch against the checkout, keeping its changes and dropping offsets and fuzz."""
    with tempfile.TemporaryDirectory() as staging:
        _stage(patch, None, Path(staging))
        if _apply(patch, Path(staging), dry_run=False).returncode:
            raise SystemExit(f"elaris: {patch.name} does not apply; resolve it by hand")
        hunks = []
        # A patch may change one file in several sections; diff each file once.
        for path in dict.fromkeys(targets(patch)):
            diff = subprocess.run(
                ("diff", "-u", "--label", f"a/{path}", "--label", f"b/{path}", str(ENGINE / path), path),
                cwd=staging, capture_output=True, text=True, check=False,
            )
            hunks.append(diff.stdout)
    patch.write_text("".join(hunks))


def check_all(ref: Optional[str] = None) -> list:
    return [check_patch(patch, ref) for patch in sorted(PATCH_DIR.glob("*.patch"))]


def report(results: list, revision: str) -> bool:
    """Print a summary table and return whether every patch applies."""
    styles = {"clean": "success", "fuzzy": "warning", "conflict": "error"}
    ui.heading(f"Ladybird patches against {revision}")
    ui.table([(r.name, f"{r.status}{f' - {r.detail}' if r.detail else ''}", styles[r.status]) for r in results])
    sys.stdout.flush()
    return all(r.status != "conflict" for r in results)


def ensure_patches_apply() -> None:
    """Stop before configuring CMake if a patch no longer applies to the checkout."""
    require("patch")
    results = check_all()
    if any(r.status == "conflict" for r in results):
        report(results, "the checkout")
        raise SystemExit("elaris: Ladybird patches conflict; fix them by hand in bridge/patches")


def _upstream_ref() -> str:
    subprocess.run(("git", "fetch", "--quiet", "origin", UPSTREAM_BRANCH), cwd=ENGINE, check=True)
    return "FETCH_HEAD"


def patches_check(args: argparse.Namespace) -> None:
    require("git", "patch")
    if not (ENGINE / ".git").exists():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    ref = _upstream_ref() if args.upstream else args.ref
    revision = "the checkout"
    if ref is not None:
        resolved = subprocess.run(
            ("git", "rev-parse", "--short", "--verify", f"{ref}^{{commit}}"),
            cwd=ENGINE, capture_output=True, text=True, check=False,
        )
        if resolved.returncode:
            raise SystemExit(f"elaris: {ref!r} is not a commit in the Ladybird checkout; fetch it first")
        revision = resolved.stdout.strip()
    if not report(check_all(ref), revision):
        raise SystemExit("elaris: some Ladybird patches no longer apply")


def patches_refresh(_args: argparse.Namespace) -> None:
    require("patch", "diff")
    fuzzy = [r.name for r in check_all() if r.status == "fuzzy"]
    for name in fuzzy:
        refresh_patch(PATCH_DIR / f"{name}.patch")
    ui.success(f"Refreshed {', '.join(fuzzy)}" if fuzzy else "Every patch already applies cleanly")


def register_commands(commands) -> None:
    patches = commands.add_parser("patches", help="Ladybird patch commands")
    subcommands = patches.add_subparsers(dest="patches_command", required=True)
    check = subcommands.add_parser("check", help="Check that every patch applies to a Ladybird revision")
    target = check.add_mutually_exclusive_group()
    target.add_argument("--ref", help="Ladybird commit, tag or branch (default: the checkout)")
    target.add_argument("--upstream", action="store_true", help=f"Fetch and check upstream {UPSTREAM_BRANCH}")
    check.set_defaults(handler=patches_check)
    refresh = subcommands.add_parser("refresh", help="Regenerate patches that apply with offset or fuzz")
    refresh.set_defaults(handler=patches_refresh)
