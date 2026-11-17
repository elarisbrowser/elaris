"""Rust application and general development commands."""

import argparse
import os
import platform
import sys

from .common import CPP_BUILD_DIR, ROOT, require, run

# Build directories that may hold the compiled bridge, most preferred first.
BRIDGE_LIB_DIRS = (CPP_BUILD_DIR / "lib", ROOT / "build" / "ladybird-integrated" / "lib")


def setup(_args: argparse.Namespace) -> None:
    require("git", "clang-format", "clang-tidy", "clangd")
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


def bridge_lib_dir() -> str:
    if configured := os.environ.get("ELARIS_LADYBIRD_LIB_DIR"):
        return configured
    for directory in BRIDGE_LIB_DIRS:
        if any(directory.glob("libelaris_ladybird.*")):
            return str(directory)
    raise SystemExit("elaris: no built Ladybird bridge found; build it as bridge/README.md describes")


def launch(args: argparse.Namespace) -> None:
    command = ["cargo", "run", "--locked"]
    if args.engine:
        os.environ["ELARIS_LADYBIRD_LIB_DIR"] = bridge_lib_dir()
        command += ["--features", "ladybird-bridge"]
    if args.url:
        command += ["--", args.url]
    run(*command)


def check(_args: argparse.Namespace) -> None:
    run("cargo", "fmt", "--check")
    run("cargo", "check", "--locked")
    run("cargo", "clippy", "--locked", "--", "-D", "warnings")
    run("cargo", "test", "--locked")


def register_commands(commands) -> None:
    for name, handler, help_text in (
        ("setup", setup, "Initialise submodules and check development tools"),
        ("build", build, "Build the Elaris Rust application"),
        ("run", launch, "Build if needed and launch Elaris"),
        ("check", check, "Run formatting, Clippy, and tests"),
    ):
        subcommand = commands.add_parser(name, help=help_text)
        subcommand.set_defaults(handler=handler)
        if handler is launch:
            subcommand.add_argument("--engine", action="store_true", help="Render pages with the built Ladybird bridge")
            subcommand.add_argument("url", nargs="?", help="Address to open, such as example.com")
