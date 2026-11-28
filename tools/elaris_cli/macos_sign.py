"""Build, sign and notarize a private macOS release from a local checkout."""

import argparse
import base64
import getpass
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import warnings
from pathlib import Path

from . import ui
from .common import CPP_BUILD_DIR, ROOT


def _credentials(certificate: str) -> dict[str, str]:
    team_id = input("Apple Developer Team ID: ").strip().upper()
    if not re.fullmatch(r"[A-Z0-9]{10}", team_id):
        raise SystemExit("Enter the 10-character Apple Developer Team ID.")
    account = input("Apple Account email for notarization: ").strip()
    if not account:
        raise SystemExit("Enter the Apple Account email for notarization.")
    with warnings.catch_warnings():
        # If echo cannot be disabled, stop instead of exposing a password.
        warnings.simplefilter("error", getpass.GetPassWarning)
        export_password = getpass.getpass(".p12 export password: ")
        app_password = getpass.getpass("Apple app-specific password: ")
    if not export_password or not app_password:
        raise SystemExit("Both passwords are required.")
    return {
        "APPLE_CERTIFICATE": certificate,
        "APPLE_CERTIFICATE_PASSWORD": export_password,
        "APPLE_TEAM_ID": team_id,
        "APPLE_ID": account,
        "APPLE_APP_PASSWORD": app_password,
    }


def _run(command, environment, **kwargs):
    return subprocess.run(command, cwd=ROOT, env=environment, check=True, **kwargs)


def sign_macos(args: argparse.Namespace) -> None:
    if sys.platform != "darwin" or platform.machine() != "arm64":
        raise SystemExit("Mac signing requires an Apple Silicon Mac and a native arm64 Python.")
    if not sys.stdin.isatty():
        raise SystemExit("Run sign-macos in an interactive terminal for password prompts.")
    try:
        _sign_macos(args)
    except (EOFError, getpass.GetPassWarning):
        raise SystemExit("Could not read credentials securely. Run sign-macos in a local terminal.") from None
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode) from None
    except OSError as error:
        raise SystemExit(str(error)) from None


def _sign_macos(args: argparse.Namespace) -> None:
    certificate_path = args.certificate.expanduser()
    if certificate_path.suffix.lower() != ".p12" or not certificate_path.is_file():
        raise SystemExit("Pass a Developer ID Application .p12 file with --certificate.")
    certificate = base64.b64encode(certificate_path.read_bytes()).decode("ascii")
    if not certificate:
        raise SystemExit("The .p12 file is empty.")
    if not shutil.which("create-dmg"):
        raise SystemExit("Install the DMG packaging tool first: brew install create-dmg")

    # Never share inherited signing inputs or the prompted secrets with build tools.
    environment = {key: value for key, value in os.environ.items() if not key.startswith("APPLE_")}
    if subprocess.run(
        ["xcrun", "--find", "notarytool"], env=environment,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ).returncode:
        raise SystemExit("Install and select full Xcode so xcrun can find notarytool.")
    if not args.skip_build:
        _run([str(ROOT / "elaris"), "bundle"], {**environment, "ELARIS_BASELINE_CPU": "1"})

    app = ROOT / "build" / "Elaris.app"
    entitlements = sorted((CPP_BUILD_DIR / "Services").glob("*/*.release.entitlements"))
    if not app.is_dir() or not entitlements:
        raise SystemExit("Could not find the app and Ladybird release entitlements. Run without --skip-build.")

    signer = [sys.executable, str(ROOT / "packaging" / "macos" / "signing.py")]
    with tempfile.TemporaryDirectory(prefix="elaris-local-signing-") as scratch:
        stage = Path(scratch)
        app_copy = stage / app.name
        staged_entitlements = stage / "entitlements"
        staged_entitlements.mkdir()
        for source in entitlements:
            name = source.name.removesuffix(".release.entitlements")
            shutil.copyfile(source, staged_entitlements / f"{name}.entitlements")
        _run(["ditto", str(app), str(app_copy)], environment)
        inputs = ["--app", str(app_copy), "--entitlements-dir", str(staged_entitlements)]
        _run([*signer, "prepare", *inputs], environment, stdout=subprocess.DEVNULL)

        signing_environment = {**environment, **_credentials(certificate)}
        destination = args.output_dir.expanduser().resolve()
        destination.mkdir(parents=True, exist_ok=True)
        output = Path(tempfile.mkdtemp(prefix="Elaris-macos-", dir=destination))
        dmg = output / "Elaris-macos-arm64.dmg"
        ui.muted(f"Output: {output}")
        ui.muted("Signing and submitting the DMG to Apple for notarization. Nothing is uploaded to GitHub.")
        _run([*signer, "release", *inputs, "--dmg", str(dmg)], signing_environment)
        ui.success(f"Signed and notarized: {dmg}")
        ui.success(f"Checksum: {dmg}.sha256")


def register_commands(commands) -> None:
    command = commands.add_parser("sign-macos", help="Sign and notarize a local macOS DMG")
    command.set_defaults(handler=sign_macos)
    command.add_argument("--certificate", required=True, type=Path, help="Developer ID Application .p12")
    command.add_argument("--skip-build", action="store_true", help="Reuse an existing app and matching entitlements")
    command.add_argument(
        "--output-dir", type=Path, default=Path("~/Downloads"),
        help="Parent of the new private output folder (default: ~/Downloads)",
    )
