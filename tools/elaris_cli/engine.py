"""Commands for building and inspecting upstream Ladybird."""

import argparse
import hashlib
import os
import subprocess
import sys
from pathlib import Path
from typing import Optional

from . import cpp, dependencies, patches, ui
from .common import CPP_BUILD_DIR, ENGINE, ROOT, VCPKG_INSTALLED, prepare_ladybird_paths, require, run_step, vcpkg_triplet

BUILD_DIR = Path(os.environ.get("LADYBIRD_BUILD_DIR", ENGINE / "Build" / "release"))
# Written after the Ladybird dependencies build, so a later run can skip them while the fingerprint still matches.
DEPENDENCIES_STAMP = BUILD_DIR / "elaris-dependencies.stamp"
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
    "WebRTCClient",
    "elaris_ladybird_smoke",
    "elaris_ladybird_download_smoke",
    "elaris_ladybird_engine_features",
)
if sys.platform == "darwin":
    # The app bundle's executable is Ladybird's own app target; the bundle step fails without it on a clean build.
    BUILD_TARGETS += ("ladybird", "ProcessReaper", "elaris_ladybird_picture_in_picture")


def dependencies_fingerprint() -> str:
    """Identifies what Ladybird's dependencies are built from: the pinned revision, Elaris's patches and the vcpkg manifests."""
    pinned = subprocess.run(
        ["git", "ls-tree", "HEAD", "vendor/ladybird"], cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    digest = hashlib.sha256(pinned.encode())
    manifests = [ENGINE / "vcpkg.json", ENGINE / "vcpkg-configuration.json"]
    manifests += sorted(path for path in (ENGINE / "Meta" / "CMake" / "vcpkg").rglob("*") if path.is_file())
    for path in [*manifests, *sorted(patches.PATCH_DIR.glob("*.patch"))]:
        if path.is_file():
            digest.update(str(path.relative_to(ROOT)).encode())
            digest.update(path.read_bytes())
    return digest.hexdigest()


def dependencies_are_current() -> bool:
    """True when the dependencies were built from the current fingerprint and their vcpkg packages are installed."""
    triplet = vcpkg_triplet()
    return (
        triplet is not None
        and (VCPKG_INSTALLED / triplet).is_dir()
        and DEPENDENCIES_STAMP.is_file()
        and DEPENDENCIES_STAMP.read_text() == dependencies_fingerprint()
    )


def build_for_elaris(jobs: Optional[str] = None) -> None:
    """Build Ladybird's dependencies, Elaris bridge, and runtime helper processes."""
    if not (ENGINE / ".git").exists():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    patches.ensure_patches_apply()
    prepare_ladybird_paths()
    require("cmake", "ninja", "rustup")
    jobs = _jobs(jobs)

    if dependencies_are_current():
        ui.muted("Ladybird dependencies match the pinned revision; skipping them.")
    else:
        build_dependencies(jobs)
    cpp.cpp_configure(argparse.Namespace())
    # A build stopped mid-write leaves a torn deps log. Ninja then drops every record after the tear on each
    # load, marks those objects stale, and rebuilds most of LibWeb on every run. Recompacting rewrites it cleanly.
    subprocess.run(["ninja", "-C", str(CPP_BUILD_DIR), "-t", "recompact"], check=False, capture_output=True)
    run_step(
        "Elaris bridge and helpers",
        "cmake",
        "--build",
        str(CPP_BUILD_DIR),
        "--parallel",
        jobs,
        "--target",
        *BUILD_TARGETS,
    )


def build_dependencies(jobs: str) -> None:
    # Removed first, so a build interrupted part way cannot leave a stamp that claims the dependencies are complete.
    DEPENDENCIES_STAMP.unlink(missing_ok=True)
    dependencies.install_rust_toolchain()
    dependencies.install_ladybird_toolchain()
    ladybird_script = str(ENGINE / "Meta" / "ladybird.py")
    run_step(
        "Ladybird vcpkg packages", sys.executable, ladybird_script, "vcpkg", "--preset", "Release", "--jobs", jobs,
        cwd=ENGINE,
    )
    run_step(
        "Ladybird dependencies", sys.executable, ladybird_script, "deps", "--preset", "Release", "--jobs", jobs,
        cwd=ENGINE,
    )
    DEPENDENCIES_STAMP.parent.mkdir(parents=True, exist_ok=True)
    DEPENDENCIES_STAMP.write_text(dependencies_fingerprint())


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
    matches = revision == checked
    ui.heading("Ladybird engine")
    ui.table([
        ("Pinned revision", revision, None),
        ("Checked out", checked, None),
        ("Revision status", "matches pin" if matches else "differs from pin", "success" if matches else "warning"),
        ("Build artifacts", f"{'present' if BUILD_DIR.is_dir() else 'absent'} ({BUILD_DIR})", None),
        ("Checkout status", "modified" if dirty else "clean", "warning" if dirty else "success"),
    ])


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
