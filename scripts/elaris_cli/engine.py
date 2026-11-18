"""Commands for building and inspecting upstream Ladybird."""

import argparse
import os
import subprocess
import sys
from pathlib import Path
from typing import Optional

from . import cpp, dependencies
from .common import CPP_BUILD_DIR, ENGINE, ROOT, prepare_ladybird_paths, require, run

BUILD_DIR = Path(os.environ.get("LADYBIRD_BUILD_DIR", ENGINE / "Build" / "release"))
BUILD_TARGETS = (
    "elaris_ladybird",
    "WebContent",
    "RequestServer",
    "Hub",
    "ImageDecoder",
    "Compositor",
    "SystemProbe",
    "AudioServer",
    "MediaServer",
    "VideoDecoderServer",
    "ProcessReaper",
)


def build_for_elaris(jobs: Optional[str] = None) -> None:
    """Build Ladybird's dependencies, Elaris bridge, and runtime helper processes."""
    if not (ENGINE / ".git").exists():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    prepare_ladybird_paths()
    require("cmake", "ninja", "rustup")
    jobs = _jobs(jobs)

    dependencies.install_rust_toolchain()
    dependencies.install_ladybird_toolchain()
    ladybird_script = str(ENGINE / "Meta" / "ladybird.py")
    run(sys.executable, ladybird_script, "vcpkg", "--preset", "Release", "--jobs", jobs, cwd=ENGINE)
    run(sys.executable, ladybird_script, "deps", "--preset", "Release", "--jobs", jobs, cwd=ENGINE)
    cpp.cpp_configure(argparse.Namespace())
    run(
        "cmake",
        "--build",
        str(CPP_BUILD_DIR),
        "--parallel",
        jobs,
        "--target",
        *BUILD_TARGETS,
    )


def _jobs(requested: Optional[str]) -> str:
    value = requested or os.environ.get("ELARIS_JOBS", "4")
    try:
        count = int(value)
    except ValueError as error:
        raise SystemExit("elaris: build job count must be a positive integer") from error
    if count < 1:
        raise SystemExit("elaris: build job count must be a positive integer")
    return str(count)


def engine_build(_args: argparse.Namespace) -> None:
    build_for_elaris(_args.jobs)


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
        ("build", engine_build, "Build Ladybird and its Elaris bridge"),
        ("status", engine_status, "Show the pinned revision and build status"),
    ):
        subcommand = subcommands.add_parser(name, help=help_text)
        subcommand.set_defaults(handler=handler)
        if handler is engine_build:
            subcommand.add_argument("--jobs", help="Parallel build jobs (default: ELARIS_JOBS or 4)")
