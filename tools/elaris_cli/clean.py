"""Remove build outputs so the next build starts from scratch."""

import argparse
import shutil
from pathlib import Path

from . import ui
from .common import ENGINE, ROOT

RUST_OUTPUTS = (ROOT / "target",)
LADYBIRD_OUTPUTS = (ROOT / "build", ENGINE / "Build")


def clean(args: argparse.Namespace) -> None:
    neither = not (args.rust or args.ladybird)
    paths: list[Path] = []
    if args.rust or neither:
        paths.extend(RUST_OUTPUTS)
    if args.ladybird or neither:
        paths.extend(LADYBIRD_OUTPUTS)

    for path in paths:
        label = path.relative_to(ROOT)
        if path.is_dir() and not path.is_symlink():
            shutil.rmtree(path)
            ui.muted(f"Removed {label}")
        elif path.exists() or path.is_symlink():
            path.unlink()
            ui.muted(f"Removed {label}")
        else:
            ui.muted(f"Nothing to remove at {label}")
    ui.success("Clean complete")


def register_commands(commands) -> None:
    subcommand = commands.add_parser(
        "clean", help="Remove build and target folders (all of them unless --rust or --ladybird is given)"
    )
    subcommand.add_argument("--rust", action="store_true", help="Remove Cargo's target folder")
    subcommand.add_argument(
        "--ladybird", action="store_true", help="Remove build/ and Ladybird's build folder"
    )
    subcommand.set_defaults(handler=clean)
