"""Rust application and general development commands."""

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

from . import dependencies, engine
from .common import (
    CPP_BUILD_DIR,
    ROOT,
    VCPKG_INSTALLED,
    bridge_files,
    prepare_ladybird_paths,
    require,
    run,
    vcpkg_triplet,
)

# Build directories that may hold the compiled bridge, most preferred first.
BRIDGE_LIB_DIRS = (CPP_BUILD_DIR / "lib", ROOT / "build" / "ladybird-integrated" / "lib")


def setup(args: argparse.Namespace) -> None:
    if not args.skip_system_deps:
        dependencies.install_system_dependencies()
    dependencies.verify_system_dependencies()
    dependencies.install_rust_toolchain()
    run("git", "submodule", "update", "--init", "--recursive")
    prepare_ladybird_paths()
    dependencies.verify_ladybird_compiler()
    dependencies.install_ladybird_toolchain()
    print("\nSetup is complete.")
    print("Next: ./elaris build  # build Ladybird and Elaris")
    print("      ./elaris run    # build if needed, then launch")
    print("The first engine build can take a while. See CONTRIBUTING.md for the full workflow.")


def build(args: argparse.Namespace) -> None:
    if args.ui:
        run("cargo", "+stable", "build", "--locked")
        return
    engine.build_for_elaris(args.jobs)
    os.environ["ELARIS_LADYBIRD_LIB_DIR"] = bridge_lib_dir()
    run("cargo", "+stable", "build", "--locked", "--features", "ladybird-bridge")


def format_sources(_args: argparse.Namespace) -> None:
    require("cargo", "clang-format")
    run("cargo", "+stable", "fmt")
    files = [str(path) for path in bridge_files()]
    if files:
        run("clang-format", "-i", *files)


def bridge_lib_dir() -> str:
    if configured := os.environ.get("ELARIS_LADYBIRD_LIB_DIR"):
        return configured
    for directory in BRIDGE_LIB_DIRS:
        if any(directory.glob("libelaris_ladybird.*")):
            return str(directory)
    raise SystemExit("elaris: no built Ladybird bridge found; build it as bridge/README.md describes")


def runtime_library_dirs(bridge_dir: str) -> list[str]:
    directories = [bridge_dir]
    if (triplet := vcpkg_triplet()) and (vcpkg_lib := VCPKG_INSTALLED / triplet / "lib").is_dir():
        directories.append(str(vcpkg_lib))
    return directories


def launch(args: argparse.Namespace) -> None:
    command = ["cargo", "+stable", "run", "--locked"]
    if not args.ui:
        engine.build_for_elaris(args.jobs)
        bridge_dir = bridge_lib_dir()
        os.environ["ELARIS_LADYBIRD_LIB_DIR"] = bridge_dir
        loader_variable = "DYLD_LIBRARY_PATH" if sys.platform == "darwin" else "LD_LIBRARY_PATH"
        runtime_dirs = runtime_library_dirs(bridge_dir)
        if existing := os.environ.get(loader_variable):
            runtime_dirs.append(existing)
        os.environ[loader_variable] = os.pathsep.join(runtime_dirs)
        command += ["--features", "ladybird-bridge"]
    if args.url:
        command += ["--", args.url]
    if args.rune_args:
        if not args.url:
            command.append("--")
        command.extend(args.rune_args)
    run(*command)


def rune_dev_launcher() -> Path:
    try:
        metadata = subprocess.run(
            ["cargo", "+stable", "metadata", "--locked", "--format-version", "1"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as error:
        raise SystemExit("elaris: cargo is required to locate Rune's hot-reload launcher") from error
    except subprocess.CalledProcessError as error:
        detail = error.stderr.strip() or str(error)
        raise SystemExit(f"elaris: could not resolve Rune's hot-reload launcher: {detail}") from error

    packages = json.loads(metadata.stdout)["packages"]
    rune = next(
        (
            package
            for package in packages
            if package["name"] == "rune"
            and (package.get("source") or "").startswith("git+https://github.com/anomalyco/rune?")
        ),
        None,
    )
    if rune is None:
        raise SystemExit("elaris: the pinned anomalyco/rune dependency was not found")

    rune_dir = Path(rune["manifest_path"]).parent
    launcher = rune_dir.parent.parent / "scripts" / "rune-dev"
    if not launcher.is_file():
        raise SystemExit(f"elaris: Rune's hot-reload launcher is missing: {launcher}")
    return launcher


def hot_reload(args: argparse.Namespace) -> None:
    command = [
        "python3",
        str(rune_dev_launcher()),
        "--local",
        "--manifest-path",
        str(ROOT / "Cargo.toml"),
        "--patch",
        "src/ui/**/*.rs",
    ]
    for pattern in args.patch:
        command.extend(("--patch", pattern))
    run(*command)


def check(_args: argparse.Namespace) -> None:
    run("cargo", "+stable", "fmt", "--check")
    run("cargo", "+stable", "check", "--locked")
    run("cargo", "+stable", "clippy", "--locked", "--", "-D", "warnings")
    run("cargo", "+stable", "test", "--locked")


def register_commands(commands) -> None:
    for name, handler, help_text in (
        ("setup", setup, "Install prerequisites and initialise Ladybird"),
        ("format", format_sources, "Format Elaris Rust and C++ source files"),
        ("build", build, "Build Ladybird, its Elaris bridge, and the browser"),
        ("run", launch, "Build if needed and launch Elaris with Ladybird"),
        ("hot", hot_reload, "Run Rune's guarded UI hot-reload workflow"),
        ("check", check, "Run formatting, Clippy, and tests"),
    ):
        subcommand = commands.add_parser(name, help=help_text)
        subcommand.set_defaults(handler=handler)
        if handler is setup:
            subcommand.add_argument(
                "--skip-system-deps",
                action="store_true",
                help="Skip package manager installs but still verify required tools",
            )
        elif handler is build:
            subcommand.add_argument("--ui", action="store_true", help="Build only the interface, without Ladybird")
            subcommand.add_argument("--jobs", help="Parallel build jobs (default: ELARIS_JOBS or 4)")
        if handler is launch:
            subcommand.add_argument(
                "--ui", action="store_true", help="Run the interface alone, without the Ladybird engine"
            )
            subcommand.add_argument("--jobs", help="Parallel build jobs (default: ELARIS_JOBS or 4)")
            subcommand.add_argument("url", nargs="?", help="Address to open, such as example.com")
            subcommand.add_argument(
                "rune_args",
                nargs=argparse.REMAINDER,
                help="Rune options after --, such as --snap /tmp/elaris.png",
            )
        if handler is hot_reload:
            subcommand.add_argument(
                "--patch",
                action="append",
                default=[],
                metavar="GLOB",
                help="Additional Rust source glob to live patch (default: src/ui/**/*.rs)",
            )
