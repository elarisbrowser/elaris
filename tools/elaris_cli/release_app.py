"""Exact cache for the unsigned Elaris release executable (not the bundle)."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

from . import packaging, release_engine
from .common import CPP_BUILD_DIR, ROOT

CACHE = ROOT / "build" / "release-app-cache"
BUILD_INFO = ROOT / "build" / "release-app-info.txt"
MANIFEST = "manifest.json"
KEY_PATTERN = re.compile(r"release-app-v1-[0-9a-f]{64}")
ENV_INPUTS = {
    "MACOSX_DEPLOYMENT_TARGET", "SDKROOT", "DEVELOPER_DIR",
    "RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER", "RUSTFLAGS",
    "CARGO_ENCODED_RUSTFLAGS", "PKG_CONFIG_PATH", "PKG_CONFIG_LIBDIR", "SOURCE_DATE_EPOCH",
}


def key(engine_key: str, channel: str) -> str:
    if not release_engine.KEY_PATTERN.fullmatch(engine_key):
        raise ValueError("no exact native engine key")
    commit = release_engine._output("git", "rev-parse", "HEAD")
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or release_engine._output(
        "git", "status", "--porcelain", "--ignore-submodules=all", "--", ".", ":(exclude).release-ccache",
    ):
        raise ValueError("Elaris executable caching requires a clean checkout")
    cargo_home = Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo"))
    inputs = {
        # build.rs embeds HEAD as well as the Rust inputs it identifies.
        "commit": commit,
        "channel": channel,
        "engine": engine_key,
        "cargo": release_engine._output("cargo", "+stable", "-V"),
        "rustc": release_engine._output("rustc", "+stable", "-vV"),
        "cargo_home_config": {
            name: release_engine._hash(cargo_home / name)
            for name in ("config", "config.toml") if (cargo_home / name).is_file()
        },
        "flags": {
            name: value for name, value in os.environ.items()
            if name in ENV_INPUTS or name.startswith(("CARGO_BUILD_", "CARGO_PROFILE_", "CARGO_TARGET_"))
        },
    }
    return "release-app-v1-" + hashlib.sha256(release_engine._json(inputs)).hexdigest()


def _bridge_digest() -> str:
    suffix = "dylib" if sys.platform == "darwin" else "so"
    return release_engine._hash(CPP_BUILD_DIR / "lib" / f"libelaris_ladybird.{suffix}")


def _binary_info(binary: Path) -> dict:
    mode = binary.lstat().st_mode
    if not stat.S_ISREG(mode) or not mode & 0o111 or mode & 0o7000 or not release_engine._elf_or_macho(binary):
        raise ValueError("cached Elaris output is not a supported executable")
    with binary.open("rb") as stream:
        header = stream.read(20)
    file_type = header[12:16] if sys.platform == "darwin" else header[16:18]
    valid_types = (b"\x02\x00\x00\x00",) if sys.platform == "darwin" else (b"\x02\x00", b"\x03\x00")
    if file_type not in valid_types:
        raise ValueError("cached Elaris output has an unexpected native file type")
    return {"mode": stat.S_IMODE(mode), "sha256": release_engine._hash(binary)}


def restore(app_key: str) -> tuple[Path, dict]:
    """Reuse only an unchanged executable linked against these exact bridge bytes."""
    manifest_path = CACHE / MANIFEST
    binary = CACHE / "elaris"
    if CACHE.is_symlink() or {path.name for path in CACHE.iterdir()} != {MANIFEST, binary.name}:
        raise ValueError("unexpected Elaris executable cache layout")
    if not stat.S_ISREG(manifest_path.lstat().st_mode) or manifest_path.stat().st_size > 4096:
        raise ValueError("invalid Elaris executable manifest")
    manifest = json.loads(manifest_path.read_text())
    if not isinstance(manifest, dict) or manifest.get("key") != app_key:
        raise ValueError("Elaris executable manifest does not match this build")
    if manifest.get("bridge") != _bridge_digest() or manifest.get("binary") != _binary_info(binary):
        raise ValueError("Elaris executable or its bridge does not match the manifest")
    for item in ("run", "attempt"):
        value = manifest.get(item)
        if value is not None and (not isinstance(value, str) or not value.isdecimal()):
            raise ValueError("invalid Elaris executable run metadata")
    return binary, manifest


def export(binary: Path, app_key: str) -> dict:
    metadata = _binary_info(binary)
    manifest = {
        "key": app_key, "bridge": _bridge_digest(), "binary": metadata,
        "run": os.environ.get("GITHUB_RUN_ID"), "attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
    }
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=CACHE.parent) as directory:
        staged = Path(directory) / CACHE.name
        staged.mkdir()
        shutil.copy2(binary, staged / "elaris")
        (staged / MANIFEST).write_bytes(release_engine._json(manifest))
        if CACHE.is_symlink():
            CACHE.unlink()
        elif CACHE.exists():
            shutil.rmtree(CACHE)
        staged.rename(CACHE)
    return manifest


def write_build_info(app_key: str, state: str, manifest: dict) -> None:
    lines = [f"Elaris executable: {state}", f"Elaris executable cache key: {app_key}"]
    if manifest.get("run"):
        lines.append(f"Elaris executable producer run/attempt: {manifest['run']}/{manifest.get('attempt') or '?'}")
    BUILD_INFO.parent.mkdir(parents=True, exist_ok=True)
    BUILD_INFO.write_text("\n".join(lines) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("engine_key")
    parser.add_argument("--canary", action="store_true")
    args = parser.parse_args()
    try:
        print(key(args.engine_key, packaging.macos_release_channel(args.canary)))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"Exact Elaris executable cache disabled; using Cargo: {error}", file=sys.stderr)
        print("")


if __name__ == "__main__":
    main()
