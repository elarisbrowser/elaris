"""C++ bridge formatting, configuration, and analysis commands."""

import argparse
import platform
import re
import subprocess

from .common import CPP_BUILD_DIR, ENGINE, ROOT, VCPKG_INSTALLED, bridge_files, require, run, vcpkg_triplet


def cpp_format(_args: argparse.Namespace) -> None:
    require("clang-format")
    files = bridge_files()
    if files:
        run("clang-format", "--dry-run", "--Werror", *(str(path) for path in files))


def cpp_configure(_args: argparse.Namespace) -> None:
    if not ENGINE.is_dir():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    require("cmake", "ninja", "rustup")
    CPP_BUILD_DIR.parent.mkdir(parents=True, exist_ok=True)
    triplet = vcpkg_triplet()
    if triplet is None:
        raise SystemExit(f"elaris: unsupported Ladybird bridge host: {platform.system()}/{platform.machine()}")
    library_suffix = "dylib" if platform.system() == "Darwin" else "so"

    prefix = VCPKG_INSTALLED / triplet
    if not prefix.is_dir():
        raise SystemExit(f"elaris: Ladybird vcpkg dependencies are not installed at {prefix}")
    toolchain_file = (ENGINE / "rust-toolchain.toml").read_text()
    toolchain_match = re.search(r'^channel\s*=\s*"([^"]+)"', toolchain_file, re.MULTILINE)
    if not toolchain_match:
        raise SystemExit(f"elaris: could not read Ladybird's Rust toolchain from {ENGINE / 'rust-toolchain.toml'}")
    rust_toolchain = toolchain_match.group(1)
    rustc = subprocess.run(
        ["rustup", "which", "--toolchain", rust_toolchain, "rustc"],
        check=False,
        capture_output=True,
        text=True,
    )
    if rustc.returncode:
        raise SystemExit(f"elaris: install Ladybird's pinned Rust toolchain {rust_toolchain}")

    harfbuzz = prefix / "lib" / f"libharfbuzz.{library_suffix}"
    harfbuzz_subset = prefix / "lib" / f"libharfbuzz-subset.{library_suffix}"
    if not harfbuzz.is_file() or not harfbuzz_subset.is_file():
        raise SystemExit(f"elaris: HarfBuzz libraries are missing from {prefix}")
    run(
        "cmake", "-S", str(ENGINE), "-B", str(CPP_BUILD_DIR), "-G", "Ninja",
        # Ladybird's own Release preset; without a build type everything compiles unoptimised.
        "-DCMAKE_BUILD_TYPE=RelWithDebInfo",
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON", "-DENABLE_GUI_TARGETS=ON",
        "-DELARIS_BUILD_SMOKE_TESTS=ON",
        f"-DELARIS_VCPKG_PREFIX={prefix}",
        f"-DCMAKE_PREFIX_PATH={prefix}",
        f"-DCMAKE_PROJECT_ladybird_INCLUDE={ROOT / 'bridge' / 'CMakeLists.txt'}",
        f"-DRUST_RUSTC={rustc.stdout.strip()}",
        f"-DHARFBUZZ_INCLUDE_DIR={prefix / 'include' / 'harfbuzz'}",
        f"-DHARFBUZZ_LIBRARY_RELEASE={harfbuzz}",
        f"-DHARFBUZZ_SUBSET_LIBRARY_RELEASE={harfbuzz_subset}",
    )


def cpp_tidy(_args: argparse.Namespace) -> None:
    require("clang-tidy")
    database = CPP_BUILD_DIR / "compile_commands.json"
    if not database.is_file():
        cpp_configure(_args)
    if not database.is_file():
        raise SystemExit(f"elaris: CMake did not generate {database}")
    sources = [path for path in bridge_files() if path.suffix in {".c", ".cc", ".cpp"}]
    if sources:
        run("clang-tidy", "-p", str(CPP_BUILD_DIR), *(str(path) for path in sources))


def register_commands(commands) -> None:
    for name, handler, help_text in (
        ("cpp-format", cpp_format, "Check formatting of Elaris-owned C++ files"),
        ("cpp-configure", cpp_configure, "Configure the bridge and export its compilation database"),
        ("cpp-tidy", cpp_tidy, "Run clang-tidy using the generated bridge compilation database"),
    ):
        subcommand = commands.add_parser(name, help=help_text)
        subcommand.set_defaults(handler=handler)
