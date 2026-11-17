"""Shared paths and process helpers for CLI commands."""

import platform
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


def bridge_files() -> list[Path]:
    directories = (ROOT / "bridge" / "src", ROOT / "bridge" / "include", ROOT / "bridge" / "tests")
    extensions = {".c", ".cc", ".cpp", ".h", ".hh", ".hpp"}
    return sorted(path for directory in directories for path in directory.rglob("*") if path.suffix in extensions)
