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
