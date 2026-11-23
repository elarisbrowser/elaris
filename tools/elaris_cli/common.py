"""Shared paths and process helpers for CLI commands."""

import os
import platform
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path
from typing import Optional

from . import ui

ROOT = Path(__file__).resolve().parents[2]
ENGINE = ROOT / "vendor" / "ladybird"
CPP_BUILD_DIR = ROOT / "build" / "cpp-ladybird"
VCPKG_INSTALLED = ENGINE / "Build" / "release" / "vcpkg_installed"
LOG_DIR = ROOT / "build" / "logs"
LOG_TAIL_LINES = 25
# Ninja and CMake report progress as "[done/total] description".
_PROGRESS = re.compile(r"\[\s*(\d+)/(\d+)\]")


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


def run_step(
    title: str,
    *command: str,
    cwd: Path = ROOT,
    position: Optional[tuple[int, int]] = None,
    log_dir: Path = LOG_DIR,
) -> None:
    """Run one build step. On an interactive terminal it shows a live spinner or progress bar and
    keeps the full output in a log file; otherwise the output streams as it always has."""
    argv = (command_path(command[0]) or command[0], *command[1:])
    slug = re.sub(r"[^a-z0-9]+", "-", title.lower()).strip("-")
    log_path = log_dir / f"{slug}.log"
    captured = ui.animated()
    started = time.monotonic()
    try:
        if captured:
            returncode = _capture(title, position, argv, cwd, log_path)
        else:
            ui.announce(title, position)
            returncode = subprocess.run(argv, cwd=cwd, check=False).returncode
    except FileNotFoundError:
        ui.step_result(title, position, time.monotonic() - started, "failed")
        raise SystemExit(f"elaris: required command '{command[0]}' was not found") from None
    except KeyboardInterrupt:
        ui.step_result(title, position, time.monotonic() - started, "interrupted")
        raise SystemExit(130) from None

    elapsed = time.monotonic() - started
    if returncode:
        ui.step_result(title, position, elapsed, "failed")
        if captured and log_path.is_file():
            tail = log_path.read_text(encoding="utf-8", errors="replace").splitlines()[-LOG_TAIL_LINES:]
            for text in tail:
                ui.muted(f"  {text}", stream=sys.stderr)
            ui.muted(f"Full log: {log_path}", stream=sys.stderr)
        raise SystemExit(returncode)
    ui.step_result(title, position, elapsed)


def _capture(
    title: str,
    position: Optional[tuple[int, int]],
    argv: tuple[str, ...],
    cwd: Path,
    log_path: Path,
) -> int:
    """Run a step with its output written to a log file and drawn as a live progress line."""
    progress = ui.Progress(title, position)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    with log_path.open("w", encoding="utf-8", errors="replace") as log:
        process = subprocess.Popen(argv, cwd=cwd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        reader = threading.Thread(target=_pump, args=(process.stdout, log, progress), daemon=True)
        progress.start()
        reader.start()
        try:
            return process.wait()
        finally:
            if process.poll() is None:  # Interrupted: stop the child before reporting.
                process.terminate()
                process.wait()
            reader.join()
            progress.stop()


def _pump(stream, log, progress: ui.Progress) -> None:
    for raw in iter(stream.readline, b""):
        text = raw.decode("utf-8", errors="replace").rstrip()
        log.write(text + "\n")
        match = _PROGRESS.search(text)
        progress.update(text, (int(match[1]), int(match[2])) if match else None)
    stream.close()


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
    if git_pointer.is_file():
        if not git_pointer.is_symlink():
            expected_pointer = f"gitdir: {git_dir_path}\n"
            if git_pointer.read_text() != expected_pointer:
                git_pointer.write_text(expected_pointer)
        main_source_dir = git_dir_path.parent.parent
        if main_source_dir.name == ".git":
            main_source_dir = main_source_dir.parent
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
