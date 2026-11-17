"""Shared paths and process helpers for CLI commands."""

import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "vendor" / "ladybird"
CPP_BUILD_DIR = ROOT / "build" / "cpp"


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
    directories = (ROOT / "bridge" / "src", ROOT / "bridge" / "include")
    extensions = {".c", ".cc", ".cpp", ".h", ".hh", ".hpp"}
    return sorted(path for directory in directories for path in directory.rglob("*") if path.suffix in extensions)
