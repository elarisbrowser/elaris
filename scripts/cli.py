#!/usr/bin/env python3
"""Small developer command wrapper for Elaris and upstream Ladybird."""

import argparse
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parent.parent
ENGINE = ROOT / "vendor" / "ladybird"
BUILD_DIR = Path(os.environ.get("LADYBIRD_BUILD_DIR", ENGINE / "Build" / "release"))


def run(*command: str, cwd: Path = ROOT) -> None:
    try:
        subprocess.run(command, cwd=cwd, check=True)
    except FileNotFoundError:
        raise SystemExit(f"elaris: required command '{command[0]}' was not found")
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from error


def require(*commands: str) -> None:
    missing = [command for command in commands if shutil.which(command) is None]
    if missing:
        raise SystemExit(f"elaris: required command(s) not found: {', '.join(missing)}")


def setup(_args: argparse.Namespace) -> None:
    require("git")
    run("git", "submodule", "update", "--init", "--recursive")
    require("cargo", "rustc")
    if sys.platform == "darwin":
        require("cmake", "ninja")
    elif sys.platform.startswith("linux"):
        require("cmake", "ninja", "pkg-config")
    else:
        raise SystemExit(f"elaris: unsupported operating system: {platform.system()}")
    print("Development tools are available. See SETUP.md for platform packages.")


def build(_args: argparse.Namespace) -> None:
    run("cargo", "build", "--locked")


def launch(_args: argparse.Namespace) -> None:
    run("cargo", "run", "--locked")


def check(_args: argparse.Namespace) -> None:
    run("cargo", "fmt", "--check")
    run("cargo", "clippy", "--locked", "--", "-D", "warnings")
    run("cargo", "test", "--locked")


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


def parser() -> argparse.ArgumentParser:
    cli = argparse.ArgumentParser(description="Developer commands for Elaris")
    commands = cli.add_subparsers(dest="command", required=True)
    for name, handler, help_text in (
        ("setup", setup, "Initialise submodules and check development tools"),
        ("build", build, "Build the Elaris Rust application"),
        ("run", launch, "Build if needed and launch Elaris"),
        ("check", check, "Run formatting, Clippy, and tests"),
    ):
        sub = commands.add_parser(name, help=help_text)
        sub.set_defaults(handler=handler)
    engine = commands.add_parser("engine", help="Ladybird engine commands")
    engine_commands = engine.add_subparsers(dest="engine_command", required=True)
    for name, handler, help_text in (
        ("build", engine_build, "Build Ladybird with its existing tooling"),
        ("status", engine_status, "Show the pinned revision and build status"),
    ):
        sub = engine_commands.add_parser(name, help=help_text)
        sub.set_defaults(handler=handler)
    return cli


def main() -> None:
    args = parser().parse_args()
    args.handler(args)


if __name__ == "__main__":
    main()
