"""Commands for building and inspecting upstream Ladybird."""

import argparse
import os
import subprocess
from pathlib import Path

from .common import ENGINE, ROOT, require, run

BUILD_DIR = Path(os.environ.get("LADYBIRD_BUILD_DIR", ENGINE / "Build" / "release"))


def engine_build(_args: argparse.Namespace) -> None:
    if not ENGINE.is_dir():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    require("cmake", "ninja")
    run("cmake", "--preset", "Release", "-B", str(BUILD_DIR), "-S", str(ENGINE))
    run("cmake", "--build", str(BUILD_DIR), "--parallel")


def engine_status(_args: argparse.Namespace) -> None:
    if not (ENGINE / ".git").exists():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    pinned = subprocess.run(
        ["git", "ls-tree", "HEAD", "vendor/ladybird"], cwd=ROOT,
        check=True, capture_output=True, text=True,
    ).stdout.split()
    checked = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=ENGINE,
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    revision = pinned[2] if len(pinned) > 2 else "unknown"
    dirty = subprocess.run(
        ["git", "status", "--porcelain"], cwd=ENGINE,
        check=True, capture_output=True, text=True,
    ).stdout.strip()
    print(f"Pinned revision: {revision}")
    print(f"Checked out:     {checked}")
    print(f"Revision status: {'matches pin' if revision == checked else 'differs from pin'}")
    print(f"Build artifacts: {'present' if BUILD_DIR.is_dir() else 'absent'} ({BUILD_DIR})")
    print(f"Checkout status: {'modified' if dirty else 'clean'}")


def register_commands(commands) -> None:
    engine = commands.add_parser("engine", help="Ladybird engine commands")
    subcommands = engine.add_subparsers(dest="engine_command", required=True)
    for name, handler, help_text in (
        ("build", engine_build, "Build Ladybird with its existing tooling"),
        ("status", engine_status, "Show the pinned revision and build status"),
    ):
        subcommand = subcommands.add_parser(name, help=help_text)
        subcommand.set_defaults(handler=handler)
