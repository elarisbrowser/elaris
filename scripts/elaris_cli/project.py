"""Rust application and general development commands."""

import argparse
import platform
import sys

from .common import require, run


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


def launch(_args: argparse.Namespace) -> None:
    run("cargo", "run", "--locked")


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
