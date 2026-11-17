"""C++ bridge formatting, configuration, and analysis commands."""

import argparse

from .common import CPP_BUILD_DIR, ENGINE, ROOT, bridge_files, require, run


def cpp_format(_args: argparse.Namespace) -> None:
    require("clang-format")
    files = bridge_files()
    if files:
        run("clang-format", "--dry-run", "--Werror", *(str(path) for path in files))


def cpp_configure(_args: argparse.Namespace) -> None:
    if not ENGINE.is_dir():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    require("cmake", "ninja")
    CPP_BUILD_DIR.parent.mkdir(parents=True, exist_ok=True)
    run(
        "cmake", "-S", str(ROOT / "bridge"), "-B", str(CPP_BUILD_DIR), "-G", "Ninja",
        "-DCMAKE_EXPORT_COMPILE_COMMANDS=ON", "-DENABLE_GUI_TARGETS=OFF",
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
