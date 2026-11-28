"""Exact cache for the native inputs to a fresh Elaris release bundle."""

from __future__ import annotations

import functools
import hashlib
import json
import os
import platform
import plistlib
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

from .common import CPP_BUILD_DIR, ENGINE, ROOT, VCPKG_INSTALLED, vcpkg_triplet

CACHE = ROOT / "build" / "release-engine-cache"
BUILD_INFO = ROOT / "build" / "release-engine-info.txt"
MANIFEST = "manifest.json"
SCHEMA = 1
# The pinned native build compiles a live Chromium preload list. Re-audit this
# input map before enabling output caching for a newer upstream revision.
AUDITED_PIN = "fb59d9637ba02998db1c20e9fec0f9cc091b70f1"
HSTS_URL = "https://raw.githubusercontent.com/chromium/chromium/main/net/http/transport_security_state_static.json"
HSTS_FILE = ROOT / "build" / "release-engine-inputs" / "transport_security_state_static.json"
KEY_PATTERN = re.compile(r"release-engine-v1-[0-9a-f]{64}")
ENV_INPUTS = (
    "ELARIS_BASELINE_CPU",
    "CC", "CXX", "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS",
    "OBJC", "OBJCXX", "OBJCFLAGS", "OBJCXXFLAGS",
    "CMAKE_PREFIX_PATH", "CMAKE_OSX_SYSROOT", "CMAKE_OSX_ARCHITECTURES",
    "CMAKE_OSX_DEPLOYMENT_TARGET", "MACOSX_DEPLOYMENT_TARGET", "SDKROOT", "DEVELOPER_DIR",
    "LAGOM_USE_LINKER", "LADYBIRD_BUILD_DIR", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
    "PKG_CONFIG_PATH", "PKG_CONFIG_LIBDIR",
    "ELARIS_RELEASE_IMAGE",
)
# Build configuration, patching and dependency setup only. App bundling, signing,
# CLI dispatch, and documentation are not native compilation inputs.
NATIVE_TOOL_FILES = (
    "tools/elaris_cli/common.py", "tools/elaris_cli/cpp.py", "tools/elaris_cli/dependencies.py",
    "tools/elaris_cli/engine.py", "tools/elaris_cli/patches.py",
)
TOOLS = ("cc", "c++", "clang", "clang++", "clang-22", "clang++-22", "cmake", "ninja", "rustc", "pkg-config", "ld", "ld.lld")
HELPERS = frozenset({
    "WebContent", "RequestServer", "Hub", "ImageDecoder", "Compositor", "SystemProbe",
    "AudioServer", "MediaServer", "VideoDecoderServer", "WebRTCClient",
})


def _output(*command: str, cwd: Path = ROOT) -> str:
    return subprocess.check_output(command, cwd=cwd, text=True).strip()


def _hash(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _json(value: object) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode()


def prepare_key() -> str:
    """Refresh mutable native build inputs once, then key the exact files CMake will read."""
    BUILD_INFO.parent.mkdir(parents=True, exist_ok=True)
    if _output("git", "rev-parse", "HEAD:vendor/ladybird") != AUDITED_PIN:
        raise ValueError("Ladybird pin has not been audited for mutable native build inputs")
    HSTS_FILE.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=HSTS_FILE.parent, delete=False) as stream:
        download = Path(stream.name)
    try:
        subprocess.run(
            ["curl", "--fail", "--location", "--silent", "--show-error", "--retry", "2", "--max-time", "30",
             "--output", str(download), HSTS_URL],
            check=True, cwd=ROOT,
        )
        if not 0 < download.stat().st_size < 16 * 1024 * 1024 or not re.search(
            rb'"entries"\s*:\s*\[', download.read_bytes(),
        ):
            raise ValueError("Chromium HSTS preload download has an unexpected format")
        download.replace(HSTS_FILE)
    finally:
        download.unlink(missing_ok=True)
    return key()


def _linux_abi() -> list:
    return [
        _output("ldd", "--version"),
        _output("dpkg", "--print-architecture"),
        Path("/etc/os-release").read_text(),
        sorted(_output("dpkg-query", "-W", "-f=${binary:Package}=${Version}\\n").splitlines()),
    ]


def key() -> str:
    """Hash native build inputs only, so ordinary browser Rust edits can reuse the engine."""
    pin = _output("git", "rev-parse", "HEAD:vendor/ladybird")
    if pin != AUDITED_PIN:
        raise ValueError("Ladybird pin has not been audited for mutable native build inputs")
    if _output("git", "rev-parse", "HEAD", cwd=ENGINE) != pin or _output(
        "git", "status", "--porcelain", "--untracked-files=normal", cwd=ENGINE,
    ):
        raise RuntimeError("Ladybird checkout must be clean and match the repository pin")
    arch = platform.machine()
    versions = {}
    binary_hash = functools.lru_cache(maxsize=None)(_hash)
    for name in TOOLS:
        if executable := shutil.which(name):
            path = Path(executable).resolve()
            if name == "ld" and sys.platform == "darwin":
                versions[name] = [executable, binary_hash(path)]
            else:
                versions[name] = [executable, _output(executable, "--version"), binary_hash(path)]
    for name in ("CC", "CXX", "OBJC", "OBJCXX"):
        if value := os.environ.get(name):
            command = shlex.split(value)
            path = shutil.which(command[0])
            if path is None:
                raise RuntimeError(f"cannot locate configured {name} compiler")
            versions[name] = [command, _output(*command, "--version"), binary_hash(Path(path).resolve())]
    if sys.platform == "darwin" and arch == "arm64":
        abi = [
            _output("xcodebuild", "-version"),
            _output("xcode-select", "-p"),
            _output("xcrun", "--sdk", "macosx", "--show-sdk-path"),
            _output("xcrun", "--sdk", "macosx", "--show-sdk-build-version"),
            _output("sw_vers", "-buildVersion"),
            sorted(_output("brew", "list", "--versions").splitlines()),
            [binary_hash(Path(_output("xcrun", "--find", name)).resolve()) for name in ("clang", "clang++", "ld")],
        ]
    elif sys.platform.startswith("linux") and arch == "x86_64":
        abi = _linux_abi()
    else:
        raise RuntimeError(f"unsupported release engine platform: {sys.platform}/{arch}")
    if os.environ.get("ELARIS_BASELINE_CPU") != "1":
        raise RuntimeError("release engine cache requires the portable CPU baseline")
    config = (ENGINE / "rust-toolchain.toml").read_text()
    pinned_rust = re.search(r'^channel\s*=\s*"([^"]+)"', config, re.MULTILINE)
    if pinned_rust is None:
        raise RuntimeError("cannot identify Ladybird's pinned Rust toolchain")

    # LibWebView gets this exact value as a CMake compile definition. Other Cargo changes
    # are handled by the Elaris executable build, not by the native engine cache.
    cargo_version = next(
        line for line in (ROOT / "Cargo.toml").read_text().splitlines() if line.startswith("version = ")
    )
    native_files = _output(
        "git", "ls-files", "-z", "--", ".gitmodules", "bridge", *NATIVE_TOOL_FILES,
    ).split("\0")
    sources = {
        name: [stat.S_IMODE((ROOT / name).stat().st_mode), _hash(ROOT / name)]
        for name in native_files if name and not name.endswith(".md")
    }
    # The helper is itself an input, including before it has been staged.
    inputs_file = Path(__file__)
    sources["tools/elaris_cli/release_engine.py"] = [stat.S_IMODE(inputs_file.stat().st_mode), _hash(inputs_file)]
    inputs = {
        "schema": SCHEMA, "pin": pin, "platform": sys.platform, "arch": arch, "triplet": vcpkg_triplet(),
        "workspace": str(ROOT), "abi": abi, "tools": versions, "sources": sources,
        "native_rust": _output("rustup", "run", pinned_rust[1], "rustc", "-vV"),
        "application_version": cargo_version, "hsts_preload": _hash(HSTS_FILE),
        "flags": {name: os.environ.get(name) for name in ENV_INPUTS},
    }
    return "release-engine-v1-" + hashlib.sha256(_json(inputs)).hexdigest()


def _layout() -> dict[Path, str]:
    triplet = vcpkg_triplet()
    if triplet is None:
        raise RuntimeError("cannot identify Ladybird's release dependency triplet")
    paths = {
        CPP_BUILD_DIR / "lib": "native",
        VCPKG_INSTALLED / triplet / "lib": "vcpkg",
    }
    if sys.platform == "darwin":
        app = CPP_BUILD_DIR / "bin" / "Ladybird.app" / "Contents"
        paths.update({app / "MacOS": "helpers", app / "Resources": "resources"})
    else:
        paths.update({CPP_BUILD_DIR / "libexec": "helpers", CPP_BUILD_DIR / "share" / "Lagom": "resources"})
    return paths


def _shared_library(path: Path) -> bool:
    return (
        path.name.endswith(".dylib") or ".so" in path.suffixes
        or any(part.endswith(".framework") for part in path.parts)
    )


def _link_inside(path: Path, root: Path) -> Path:
    target = Path(os.readlink(path))
    if target.is_absolute() or not path.resolve(strict=True).is_relative_to(root.resolve()):
        raise ValueError(f"nonportable symlink in engine output: {path}")
    return target


def _inventory(root: Path) -> dict[str, list]:
    files = {}
    for path in sorted(root.rglob("*")):
        relative = path.relative_to(root).as_posix()
        if relative == MANIFEST:
            continue
        mode = path.lstat().st_mode
        if stat.S_ISLNK(mode):
            group = root / path.relative_to(root).parts[0]
            if path == group:
                raise ValueError(f"invalid engine output directory: {relative}")
            files[relative] = ["symlink", str(_link_inside(path, group))]
        elif stat.S_ISREG(mode):
            files[relative] = ["file", stat.S_IMODE(mode), _hash(path)]
        elif stat.S_ISDIR(mode):
            files[relative] = ["directory", stat.S_IMODE(mode)]
        else:
            raise ValueError(f"unsupported file in engine output: {relative}")
    return files


def _check_layout(root: Path) -> None:
    helpers = HELPERS
    suffix = "so"
    expected = {"native", "vcpkg", "helpers", "resources"}
    if sys.platform == "darwin":
        helpers |= {"Ladybird", "ProcessReaper"}
        suffix = "dylib"
        expected.add("entitlements")
        for helper in sorted(helpers - {"Ladybird"}):
            path = root / "entitlements" / helper / f"{helper}.entitlements"
            if not path.is_file() or not isinstance(plistlib.loads(path.read_bytes()), dict):
                raise ValueError(f"generated entitlements are missing or invalid for {helper}")
    if {path.name for path in root.iterdir()} - {MANIFEST} != expected:
        raise ValueError("missing or unexpected directory in the cached engine")
    for group in expected:
        path = root / group
        if path.is_symlink() or not path.is_dir():
            raise ValueError(f"invalid engine output directory: {group}")
    for helper in sorted(helpers):
        path = root / "helpers" / helper
        if not _elf_or_macho(path) or not path.stat().st_mode & 0o111:
            raise ValueError(f"invalid native helper: {helper}")
    if not _elf_or_macho(root / "native" / f"libelaris_ladybird.{suffix}"):
        raise ValueError("cached Elaris bridge is missing or not a native library")
    for name in ("resources", "vcpkg"):
        if not any(path.is_file() for path in (root / name).rglob("*")):
            raise ValueError(f"empty engine output: {name}")


def _elf_or_macho(path: Path) -> bool:
    if not path.is_file() or path.stat().st_size < 20:
        return False
    with path.open("rb") as stream:
        header = stream.read(20)
    if sys.platform == "darwin":
        # Release uses native thin arm64 builds, not a host-generic CPU format.
        return header[:8] == b"\xcf\xfa\xed\xfe\x0c\x00\x00\x01"
    return header[:6] == b"\x7fELF\x02\x01" and header[18:20] == b"\x3e\x00"


def _copy_libraries(source: Path, destination: Path) -> None:
    destination.mkdir()
    for entry in source.iterdir():
        if not _shared_library(entry):
            continue
        target = destination / entry.name
        if entry.is_symlink():
            target.symlink_to(_link_inside(entry, source))
        elif entry.is_dir():
            shutil.copytree(entry, target, symlinks=True)
        else:
            shutil.copy2(entry, target)


def _service_entitlements():
    return (CPP_BUILD_DIR / "Services").glob("*/*.entitlements")


def export(engine_key: str) -> dict:
    _validate_key(engine_key)
    if CACHE.exists():
        shutil.rmtree(CACHE)
    CACHE.mkdir(parents=True)
    for source, group in _layout().items():
        target = CACHE / group
        if group in ("native", "vcpkg"):
            _copy_libraries(source, target)
        else:
            shutil.copytree(source, target, symlinks=True)
    if sys.platform == "darwin":
        target = CACHE / "entitlements"
        target.mkdir()
        for source in _service_entitlements():
            destination = target / source.parent.name / source.name
            destination.parent.mkdir(exist_ok=True)
            shutil.copy2(source, destination)
    _check_layout(CACHE)
    manifest = {"schema": SCHEMA, "key": engine_key, "files": _inventory(CACHE), **provenance()}
    (CACHE / MANIFEST).write_bytes(_json(manifest))
    return manifest


def provenance() -> dict:
    return {
        "producer": _output("git", "rev-parse", "HEAD"),
        "run": os.environ.get("GITHUB_RUN_ID"),
        "attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
    }


def _validate_key(engine_key: str) -> None:
    if not KEY_PATTERN.fullmatch(engine_key):
        raise ValueError("invalid release engine key")


def restore(engine_key: str) -> dict:
    """Validate the whole payload before making any native files available to Cargo."""
    _validate_key(engine_key)
    if CACHE.is_symlink() or (CACHE / MANIFEST).is_symlink():
        raise ValueError("invalid symlink in the release engine cache")
    manifest = json.loads((CACHE / MANIFEST).read_text())
    if manifest["schema"] != SCHEMA or manifest["key"] != engine_key:
        raise ValueError("release engine manifest does not match this build")
    if not re.fullmatch(r"[0-9a-f]{40}", manifest["producer"]):
        raise ValueError("release engine manifest has invalid provenance")
    if any(value is not None and (not isinstance(value, str) or not value.isdecimal())
           for value in (manifest.get("run"), manifest.get("attempt"))):
        raise ValueError("release engine manifest has invalid run metadata")
    if manifest["files"] != _inventory(CACHE):
        raise ValueError("release engine cache is incomplete or changed")
    _check_layout(CACHE)
    layout = _layout()
    if sys.platform == "darwin":
        layout[CPP_BUILD_DIR / "Services"] = "entitlements"
    if any(path.exists() or path.is_symlink() for path in layout):
        raise RuntimeError("cannot restore engine outputs onto an existing native build")
    try:
        for destination, group in layout.items():
            shutil.copytree(CACHE / group, destination, symlinks=True)
    except OSError as error:
        # Installation errors are not ordinary cache misses: stop rather than build on half-restored files.
        raise RuntimeError(f"cannot install verified release engine output: {error}") from error
    return manifest


def write_build_info(engine_key: str, state: str, manifest: dict) -> None:
    BUILD_INFO.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        f"Native engine: {state}",
        f"Native engine cache key: {engine_key}",
        f"Native engine producer commit: {manifest['producer']}",
        f"Native engine HSTS preload SHA-256: {_hash(HSTS_FILE)}",
    ]
    if manifest.get("run"):
        lines.append(f"Native engine producer run/attempt: {manifest['run']}/{manifest.get('attempt') or '?'}")
    if manifest.get("files"):
        lines.append(f"Native engine output manifest SHA-256: {hashlib.sha256(_json(manifest)).hexdigest()}")
    BUILD_INFO.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    if sys.argv[1:] != ["prepare"]:
        raise SystemExit("usage: python3 -m tools.elaris_cli.release_engine prepare")
    try:
        print(prepare_key())
    except (RuntimeError, ValueError, OSError, subprocess.CalledProcessError, StopIteration) as error:
        # A network or toolchain probe failure must not disable an otherwise valid cold release.
        print(f"Exact Ladybird cache disabled; using the normal native build: {error}", file=sys.stderr)
        print("")
