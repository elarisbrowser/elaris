"""Install and verify the host tools needed to build Elaris and Ladybird."""

import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Optional

from .common import ROOT, command_path, require, run

MACOS_PACKAGES = (
    "autoconf",
    "autoconf-archive",
    "automake",
    "ccache",
    "cmake",
    "libtool",
    "make",
    "nasm",
    "ninja",
    "pkg-config",
    "llvm@22",
)

DEBIAN_PACKAGES = (
    "autoconf",
    "autoconf-archive",
    "automake",
    "ca-certificates",
    "build-essential",
    "ccache",
    "cmake",
    "curl",
    "fonts-liberation2",
    "git",
    "gnupg",
    "glslang-tools",
    "libdrm-dev",
    "libgl1-mesa-dev",
    "libncurses-dev",
    "libpulse-dev",
    "libtool",
    "libwayland-dev",
    "libxcb-icccm4-dev",
    "libxkbcommon-dev",
    "nasm",
    "ninja-build",
    "pkg-config",
    "python3-venv",
    "qt6-base-private-dev",
    "qt6-positioning-dev",
    "qt6-tools-dev-tools",
    "qt6-wayland",
    "tar",
    "unzip",
    "zip",
    "lsb-release",
)

FEDORA_PACKAGES = (
    "autoconf-archive",
    "automake",
    "ccache",
    "cmake",
    "curl",
    "gcc-c++",
    "git",
    "glslang",
    "libdrm-devel",
    "liberation-sans-fonts",
    "libglvnd-devel",
    "libpulse-devel",
    "libxkbcommon-devel",
    "libtool",
    "nasm",
    "ncurses-devel",
    "ninja-build",
    "patchelf",
    "perl-FindBin",
    "perl-IPC-Cmd",
    "perl-lib",
    "perl-Time-Piece",
    "pkgconf-pkg-config",
    "qt6-qtbase-private-devel",
    "qt6-qtpositioning-devel",
    "qt6-qttools-devel",
    "qt6-qtwayland-devel",
    "tar",
    "unzip",
    "xcb-util-wm-devel",
    "zip",
    "zlib-ng-compat-static",
    "clang",
    "clang-tools-extra",
)

ARCH_PACKAGES = (
    "autoconf-archive",
    "base-devel",
    "ccache",
    "cmake",
    "curl",
    "git",
    "libgl",
    "libpulse",
    "libxkbcommon",
    "nasm",
    "ninja",
    "python",
    "qt6-base",
    "qt6-positioning",
    "qt6-tools",
    "qt6-wayland",
    "ttf-liberation",
    "wayland",
    "xcb-util-wm",
    "tar",
    "unzip",
    "zip",
    "clang",
)

OPENSUSE_PACKAGES = (
    "autoconf-archive",
    "automake",
    "ccache",
    "cmake",
    "curl",
    "gcc16",
    "gcc16-c++",
    "git",
    "liberation-fonts",
    "libglvnd-devel",
    "libpulse-devel",
    "libxkbcommon-devel",
    "libtool",
    "nasm",
    "ncurses-devel",
    "ninja",
    "pkg-config",
    "qt6-base-private-devel",
    "qt6-positioning-devel",
    "qt6-tools-devel",
    "qt6-wayland-devel",
    "tar",
    "unzip",
    "xcb-util-wm-devel",
    "zip",
    "clang",
    "clang-tools",
)


def install_system_dependencies() -> None:
    """Install upstream Ladybird build packages through the host package manager."""
    system = platform.system()
    if system == "Darwin":
        if not shutil.which("brew"):
            raise SystemExit(
                "elaris: Homebrew is required for automatic macOS setup. Install Homebrew, then rerun ./elaris setup."
            )
        _verify_xcode_version()
        run("brew", "install", *MACOS_PACKAGES)
        return

    if system != "Linux":
        raise SystemExit(f"elaris: automatic setup is not supported on {system}; see SETUP.md")

    package_manager = _linux_package_manager()
    if package_manager is None:
        raise SystemExit(
            "elaris: no supported Linux package manager found (apt, dnf, pacman, or zypper); "
            "install the prerequisites from SETUP.md and rerun ./elaris setup --skip-system-deps"
        )

    packages_by_manager = {
        "apt-get": DEBIAN_PACKAGES,
        "dnf": FEDORA_PACKAGES,
        "pacman": ARCH_PACKAGES,
        "zypper": OPENSUSE_PACKAGES,
    }
    privileged = _privileged_prefix()
    if package_manager == "apt-get":
        _install_apt_dependencies(privileged, packages_by_manager[package_manager])
    elif package_manager == "dnf":
        run(*privileged, "dnf", "install", "-y", *packages_by_manager[package_manager])
    elif package_manager == "pacman":
        run(*privileged, "pacman", "-S", "--needed", "--noconfirm", *packages_by_manager[package_manager])
    else:
        run(*privileged, "zypper", "refresh")
        run(*privileged, "zypper", "--non-interactive", "install", *packages_by_manager[package_manager])


def _install_apt_dependencies(privileged: tuple[str, ...], packages: tuple[str, ...]) -> None:
    run(*privileged, "apt-get", "update")
    run(*privileged, "apt-get", "install", "-y", *packages)

    codename = subprocess.run(["lsb_release", "-sc"], capture_output=True, text=True, check=False)
    if codename.returncode:
        raise SystemExit("elaris: could not determine the Linux release codename for LLVM's package repository")
    release = codename.stdout.strip()
    if not release:
        raise SystemExit("elaris: Linux release codename is empty; cannot configure LLVM's package repository")

    with tempfile.TemporaryDirectory(prefix="elaris-apt-") as temporary_directory:
        key_file = Path(temporary_directory) / "llvm-snapshot.gpg.key"
        source_file = Path(temporary_directory) / "elaris-llvm-22.list"
        run(
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "https://apt.llvm.org/llvm-snapshot.gpg.key",
            "--output",
            str(key_file),
        )
        run(
            *privileged,
            "install",
            "-D",
            "-m",
            "0644",
            str(key_file),
            "/usr/share/keyrings/llvm-snapshot.gpg.key",
        )
        source_file.write_text(
            f"deb [signed-by=/usr/share/keyrings/llvm-snapshot.gpg.key] "
            f"https://apt.llvm.org/{release}/ llvm-toolchain-{release}-22 main\n"
        )
        run(
            *privileged,
            "install",
            "-D",
            "-m",
            "0644",
            str(source_file),
            "/etc/apt/sources.list.d/elaris-llvm-22.list",
        )
    run(*privileged, "apt-get", "update")
    run(
        *privileged,
        "apt-get",
        "install",
        "-y",
        "clang-22",
        "clangd-22",
        "clang-tools-22",
        "clang-format-22",
        "clang-tidy-22",
        "lld-22",
    )

    if _cmake_version_is_current():
        return
    if _linux_distribution_id() != "ubuntu":
        return

    with tempfile.TemporaryDirectory(prefix="elaris-kitware-") as temporary_directory:
        key_file = Path(temporary_directory) / "kitware-archive-latest.asc"
        source_file = Path(temporary_directory) / "kitware.list"
        run(
            "curl",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "https://apt.kitware.com/keys/kitware-archive-latest.asc",
            "--output",
            str(key_file),
        )
        run(
            *privileged,
            "gpg",
            "--dearmor",
            "--yes",
            "--output",
            "/usr/share/keyrings/kitware-archive-keyring.gpg",
            str(key_file),
        )
        source_file.write_text(
            f"deb [signed-by=/usr/share/keyrings/kitware-archive-keyring.gpg] "
            f"https://apt.kitware.com/ubuntu/ {release} main\n"
        )
        run(
            *privileged,
            "install",
            "-D",
            "-m",
            "0644",
            str(source_file),
            "/etc/apt/sources.list.d/kitware.list",
        )
    run(*privileged, "apt-get", "update")
    run(*privileged, "apt-get", "install", "-y", "cmake")


def _linux_distribution_id() -> str:
    release = Path("/etc/os-release")
    if not release.is_file():
        return ""
    match = re.search(r"^ID=(?:\"([^\"]+)\"|'([^']+)'|([^\s]+))", release.read_text(errors="replace"), re.MULTILINE)
    return next((value for value in match.groups() if value), "") if match else ""


def _cmake_version_is_current() -> bool:
    cmake = command_path("cmake")
    if not cmake:
        return False
    result = subprocess.run([cmake, "--version"], capture_output=True, text=True, check=False)
    match = re.search(r"version\s+(\d+)\.(\d+)(?:\.(\d+))?", result.stdout)
    return bool(match and result.returncode == 0 and tuple(int(part or 0) for part in match.groups()) >= (3, 30, 0))


def _linux_package_manager() -> Optional[str]:
    release = Path("/etc/os-release")
    os_release = release.read_text(errors="replace") if release.is_file() else ""
    raw_ids = re.findall(r"^(?:ID|ID_LIKE)=(?:\"([^\"]+)\"|'([^']+)'|([^\s]+))", os_release, re.MULTILINE)
    ids = {identifier for group in raw_ids for value in group if value for identifier in value.split()}

    preferences = ("apt-get", "dnf", "pacman", "zypper")
    if "debian" in ids or "ubuntu" in ids:
        preferences = ("apt-get", *preferences[1:])
    elif "fedora" in ids:
        preferences = ("dnf", "apt-get", "pacman", "zypper")
    elif "arch" in ids or "manjaro" in ids:
        preferences = ("pacman", "apt-get", "dnf", "zypper")
    elif "opensuse" in ids or "suse" in ids:
        preferences = ("zypper", "apt-get", "dnf", "pacman")
    return next((manager for manager in preferences if shutil.which(manager)), None)


def _privileged_prefix() -> tuple[str, ...]:
    if os.geteuid() == 0:
        return ()
    if not shutil.which("sudo"):
        raise SystemExit("elaris: sudo is required to install Linux system packages")
    run("sudo", "-v")
    return ("sudo",)


def install_rust_toolchain() -> None:
    """Ensure rustup and the project's stable Rust toolchain are available to child commands."""
    cargo_bin = Path.home() / ".cargo" / "bin"
    if cargo_bin.is_dir() and str(cargo_bin) not in os.environ.get("PATH", "").split(os.pathsep):
        os.environ["PATH"] = f"{cargo_bin}{os.pathsep}{os.environ.get('PATH', '')}"

    if not command_path("rustup"):
        require("curl", "sh")
        with tempfile.TemporaryDirectory(prefix="elaris-rustup-") as temporary_directory:
            installer = Path(temporary_directory) / "rustup-init.sh"
            run(
                "curl",
                "--proto",
                "=https",
                "--tlsv1.2",
                "-sSf",
                "https://sh.rustup.rs",
                "-o",
                str(installer),
            )
            run("sh", str(installer), "-y", "--profile", "minimal", "--component", "clippy", "--component", "rustfmt")
        if cargo_bin.is_dir():
            os.environ["PATH"] = f"{cargo_bin}{os.pathsep}{os.environ.get('PATH', '')}"

    require("rustup")
    _ensure_rust_toolchain("stable")


def install_ladybird_toolchain() -> None:
    """Install the Rust toolchain pinned by the Ladybird submodule."""
    toolchain_file = ROOT / "vendor" / "ladybird" / "rust-toolchain.toml"
    if not toolchain_file.is_file():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    match = re.search(r'^channel\s*=\s*"([^"]+)"', toolchain_file.read_text(), re.MULTILINE)
    if not match:
        raise SystemExit(f"elaris: could not read Ladybird's Rust toolchain from {toolchain_file}")
    _ensure_rust_toolchain(match.group(1))


def verify_ladybird_compiler() -> None:
    """Run Ladybird's compiler selector so an unsupported C++ toolchain fails during setup."""
    compiler_script = ROOT / "vendor" / "ladybird" / "Meta" / "Utils" / "find_compiler.py"
    if not compiler_script.is_file():
        raise SystemExit("elaris: Ladybird submodule is missing; run ./elaris setup")
    run(sys.executable, str(compiler_script))


def _ensure_rust_toolchain(channel: str) -> None:
    components = ("clippy", "rustfmt")
    installed = subprocess.run(["rustup", "toolchain", "list"], capture_output=True, text=True, check=False)
    if installed.returncode:
        raise SystemExit(installed.returncode)
    toolchains = {line.split()[0] for line in installed.stdout.splitlines() if line.split()}
    if not any(name == channel or name.startswith(f"{channel}-") for name in toolchains):
        run(
            "rustup",
            "toolchain",
            "install",
            channel,
            "--profile",
            "minimal",
            "--component",
            components[0],
            "--component",
            components[1],
        )
        return

    installed_components = subprocess.run(
        ["rustup", "component", "list", "--toolchain", channel, "--installed"],
        capture_output=True,
        text=True,
        check=False,
    )
    if installed_components.returncode:
        raise SystemExit(installed_components.returncode)
    present = {line.split("-", 1)[0] for line in installed_components.stdout.splitlines()}
    missing = [component for component in components if component not in present]
    if missing:
        run("rustup", "component", "add", *missing, "--toolchain", channel)


def verify_system_dependencies() -> None:
    system = platform.system()
    if system not in {"Darwin", "Linux"}:
        raise SystemExit(f"elaris: setup is supported on macOS and Linux, not {system}")
    if system == "Darwin":
        _verify_xcode_version()
    require("git", "cmake", "ninja", "pkg-config", "clang-format", "clang-tidy")
    _verify_cmake_version()
    if system == "Linux":
        _verify_qt_version()


def _verify_cmake_version() -> None:
    result = subprocess.run(
        [command_path("cmake") or "cmake", "--version"], capture_output=True, text=True, check=False
    )
    match = re.search(r"version\s+(\d+)\.(\d+)(?:\.(\d+))?", result.stdout)
    if result.returncode or not match:
        raise SystemExit(
            "elaris: could not determine the installed CMake version; Ladybird requires CMake 3.30 or newer"
        )
    version = tuple(int(part or 0) for part in match.groups())
    if version < (3, 30, 0):
        found = ".".join(str(part) for part in version)
        raise SystemExit(
            f"elaris: CMake {found} is too old; Ladybird requires CMake 3.30 or newer. "
            "Install a newer CMake, then rerun ./elaris setup."
        )


def _verify_qt_version() -> None:
    candidates = (
        ("pkg-config", "--modversion", "Qt6Core"),
        ("qtpaths6", "--qt-version"),
        ("qmake6", "-query", "QT_VERSION"),
    )
    version = None
    for command in candidates:
        if not command_path(command[0]):
            continue
        result = subprocess.run(command, capture_output=True, text=True, check=False)
        if result.returncode == 0:
            version = result.stdout.strip().splitlines()[0]
            break
    match = re.match(r"(\d+)\.(\d+)", version or "")
    if not match or tuple(int(part) for part in match.groups()) < (6, 10):
        found = version or "not found"
        raise SystemExit(
            f"elaris: Qt 6.10 or newer is required to build Ladybird (found {found}). "
            "Install a current Qt 6 development kit, then rerun ./elaris setup."
        )


def _verify_xcode_version() -> None:
    xcode_select = shutil.which("xcode-select")
    if not xcode_select:
        raise SystemExit("elaris: Xcode 26 or newer is required, including its command-line tools.")
    developer_dir = subprocess.run([xcode_select, "-p"], capture_output=True, text=True, check=False)
    if (
        developer_dir.returncode
        or "/Contents/Developer" not in developer_dir.stdout.strip()
        or "CommandLineTools" in developer_dir.stdout
    ):
        raise SystemExit(
            "elaris: select the full Xcode installation with xcode-select before building Ladybird."
        )
    if not command_path("xcodebuild"):
        raise SystemExit(
            "elaris: Xcode 26 or newer is required. Install Xcode, select it with xcode-select, then rerun setup."
        )
    result = subprocess.run(
        [command_path("xcodebuild") or "xcodebuild", "-version"], capture_output=True, text=True, check=False
    )
    match = re.search(r"^Xcode\s+(\d+)", result.stdout, re.MULTILINE)
    if result.returncode or not match or int(match.group(1)) < 26:
        found = match.group(1) if match else "unknown"
        raise SystemExit(f"elaris: Xcode 26 or newer is required to build Ladybird (found {found}).")
