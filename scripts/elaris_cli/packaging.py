"""Assemble Elaris's app bundles: a branded .app on macOS and a desktop-entry tree on Linux."""

import filecmp
import shutil
import subprocess
from pathlib import Path

from .common import ROOT, run

ASSETS = ROOT / "assets"
LINUX_ASSETS = ASSETS / "linux"
# Compiled from the brand kit's Icon Composer document, so the designer's layers and gradient carry over.
APP_ICON_SOURCE = ASSETS / "brand-kit" / "apple" / "Icon Exports" / "Icon.icon"
APP_ID = "org.elarisbrowser.Elaris"


def _copy_if_changed(source: Path, destination: Path) -> bool:
    """Copy `source` over `destination` when their contents differ. Returns whether it copied.

    The copy takes a fresh modification time on purpose: Launch Services caches an app's icon by file time,
    so a copy that kept the source's older time can keep showing a stale icon in the Dock.
    """
    if destination.exists() and filecmp.cmp(source, destination, shallow=False):
        return False
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    shutil.copymode(source, destination)
    return True


def _macos_app_icon() -> Path:
    """Compile the icon into Assets.car (macOS 26+) and Icon.icns (older systems), cached under build/."""
    output = ROOT / "build" / "icons" / "macos"
    compiled = [output / "Assets.car", output / "Icon.icns"]
    newest_source = max(path.stat().st_mtime_ns for path in APP_ICON_SOURCE.rglob("*") if path.is_file())
    if all(path.is_file() and path.stat().st_mtime_ns >= newest_source for path in compiled):
        return output

    output.mkdir(parents=True, exist_ok=True)
    run(
        "xcrun", "actool", str(APP_ICON_SOURCE),
        "--compile", str(output),
        "--output-partial-info-plist", str(output / "partial-info.plist"),
        "--app-icon", "Icon",
        "--platform", "macosx",
        "--target-device", "mac",
        "--minimum-deployment-target", "26.0",
        "--development-region", "en",
        "--enable-on-demand-resources", "NO",
        "--output-format", "human-readable-text",
        "--errors", "--warnings",
    )
    return output


def macos_app_bundle(executable: Path, name: str = "Elaris") -> Path:
    """Copy the executable, Info.plist and icon into build/<name>.app and sign it. Returns the bundled executable."""
    contents = ROOT / "build" / f"{name}.app" / "Contents"
    icon = _macos_app_icon()
    bundled_executable = contents / "MacOS" / executable.name
    copies = [
        (ASSETS / "Info.plist", contents / "Info.plist"),
        (executable, bundled_executable),
        (icon / "Assets.car", contents / "Resources" / "Assets.car"),
        (icon / "Icon.icns", contents / "Resources" / "Icon.icns"),
    ]
    # Every copy runs, so the list rather than `any` over a generator.
    copied = [_copy_if_changed(source, destination) for source, destination in copies]
    if any(copied):
        # Replacing any file in a signed bundle invalidates its signature, and macOS kills the app for that.
        subprocess.run(["codesign", "--force", "--sign", "-", str(contents.parent)], check=True)
    return bundled_executable


def linux_desktop_bundle(executable: Path, prefix: Path) -> Path:
    """Lay out bin/ and share/ under `prefix`, so copying it into ~/.local or /usr installs Elaris with its icon."""
    installed = prefix / "bin" / executable.name
    _copy_if_changed(executable, installed)
    _copy_if_changed(LINUX_ASSETS / f"{APP_ID}.desktop", prefix / "share" / "applications" / f"{APP_ID}.desktop")
    icon_root = LINUX_ASSETS / "icons"
    for source in icon_root.rglob("*"):
        if source.is_file():
            _copy_if_changed(source, prefix / "share" / "icons" / source.relative_to(icon_root))
    return installed
