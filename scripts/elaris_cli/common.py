"""Shared paths and process helpers for CLI commands."""

import os
import platform
import re
import shutil
import subprocess
from pathlib import Path
from typing import Optional

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "vendor" / "ladybird"
CPP_BUILD_DIR = ROOT / "build" / "cpp-ladybird"
VCPKG_INSTALLED = ENGINE / "Build" / "release" / "vcpkg_installed"


def vcpkg_triplet() -> Optional[str]:
    """Ladybird's vcpkg triplet for this host, or None on a host the bridge does not support."""
    os_name = {"Darwin": "osx", "Linux": "linux"}.get(platform.system())
    if os_name is None:
        return None
    arch = "arm64" if platform.machine().lower() in {"arm64", "aarch64"} else "x64"
    return f"{arch}-{os_name}-dynamic"


def run(*command: str, cwd: Path = ROOT) -> None:
    executable = command_path(command[0]) or command[0]
    try:
        subprocess.run((executable, *command[1:]), cwd=cwd, check=True)
    except FileNotFoundError:
        raise SystemExit(f"elaris: required command '{command[0]}' was not found")
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from error


def require(*commands: str) -> None:
    missing = [command for command in commands if command_path(command) is None]
    if missing:
        raise SystemExit(f"elaris: required command(s) not found: {', '.join(missing)}")


def prepare_ladybird_paths() -> None:
    """Use absolute submodule paths for Ladybird's shared vcpkg build tree."""
    git_pointer = ENGINE / ".git"
    if not git_pointer.exists():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")

    git_dir = subprocess.run(
        ["git", "rev-parse", "--absolute-git-dir"],
        cwd=ENGINE,
        check=False,
        capture_output=True,
        text=True,
    )
    if git_dir.returncode:
        raise SystemExit(f"elaris: could not locate the Ladybird Git directory: {git_dir.stderr.strip()}")

    git_dir_path = Path(git_dir.stdout.strip())
    if git_pointer.is_file() and not git_pointer.is_symlink():
        expected_pointer = f"gitdir: {git_dir_path}\n"
        if git_pointer.read_text() != expected_pointer:
            git_pointer.write_text(expected_pointer)
        main_source_dir = git_dir_path.parent.parent
    else:
        main_source_dir = ENGINE

    os.environ["LADYBIRD_MAIN_SOURCE_DIR"] = str(main_source_dir)
    os.environ["LADYBIRD_CACHE_DIR"] = str(main_source_dir / "Build" / "caches")
    os.environ["VCPKG_ROOT"] = str(main_source_dir / "Build" / "vcpkg-release")


def command_path(command: str) -> Optional[str]:
    """Find a command, including versioned LLVM tools and Homebrew's keg-only LLVM tools."""
    if command not in {"clang-format", "clang-tidy", "clangd"}:
        return shutil.which(command)

    if platform.system() == "Darwin":
        brew = shutil.which("brew")
        if brew:
            prefix = subprocess.run(
                [brew, "--prefix", "llvm@22"], capture_output=True, text=True, check=False
            )
            candidate = Path(prefix.stdout.strip()) / "bin" / command
            if prefix.returncode == 0 and candidate.is_file():
                return str(candidate)

    pattern = re.compile(rf"^{re.escape(command)}-(\d+(?:\.\d+)*)$")
    candidates: list[tuple[tuple[int, ...], str]] = []
    for directory in os.environ.get("PATH", "").split(os.pathsep):
        if not directory:
            continue
        for candidate in Path(directory).glob(f"{command}-*"):
            match = pattern.match(candidate.name)
            if match and candidate.is_file():
                candidates.append((tuple(int(part) for part in match.group(1).split(".")), str(candidate)))
    if candidates:
        return max(candidates)[1]
    return shutil.which(command)


def bridge_files() -> list[Path]:
    directories = (ROOT / "bridge" / "src", ROOT / "bridge" / "include", ROOT / "bridge" / "tests")
    extensions = {".c", ".cc", ".cpp", ".h", ".hh", ".hpp"}
    return sorted(path for directory in directories for path in directory.rglob("*") if path.suffix in extensions)
