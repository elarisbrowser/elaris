"""Exercise the local signing command without Apple credentials or tools."""

import base64
import getpass
import os
import shutil
import subprocess
import tempfile
import unittest
import warnings
from pathlib import Path
from unittest.mock import patch

from cli import parser
from elaris_cli import macos_sign


class LocalSigningTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enter_context(tempfile.TemporaryDirectory()))
        self.certificate = self.root / "Developer ID.p12"
        self.certificate.write_bytes(b"mock p12")
        self.output = self.root / "output"
        self.app = self.root / "build" / "Elaris.app"
        self.cpp = self.root / "build" / "cpp-ladybird"
        self.entitlements = self.cpp / "Services" / "WebContent" / "WebContent.release.entitlements"
        self.calls = []
        self.signing_failure = False

    # unittest.TestCase.enterContext is only available in Python 3.11.
    def enter_context(self, manager):
        result = manager.__enter__()
        self.addCleanup(manager.__exit__, None, None, None)
        return result

    def run_cli(self, *options):
        args = parser().parse_args([
            "sign-macos", "--certificate", str(self.certificate), "--output-dir", str(self.output), *options,
        ])
        args.handler(args)

    def make_bundle(self):
        self.app.mkdir(parents=True, exist_ok=True)
        (self.app / "untouched").write_text("original")
        self.entitlements.parent.mkdir(parents=True, exist_ok=True)
        self.entitlements.write_text("release entitlements")

    def run_process(self, command, **kwargs):
        self.calls.append((command, kwargs["env"]))
        if command[-1] == "bundle":
            self.make_bundle()
        elif command[0] == "ditto":
            shutil.copytree(command[1], command[2])
        elif "release" in command:
            staged_app = Path(command[command.index("--app") + 1])
            staged_entitlements = Path(command[command.index("--entitlements-dir") + 1])
            self.assertNotEqual(staged_app, self.app)
            self.assertEqual((staged_entitlements / "WebContent.entitlements").read_text(), "release entitlements")
            (staged_app / "untouched").write_text("signed")
            dmg = Path(command[command.index("--dmg") + 1])
            dmg.write_text("mock dmg")
            if self.signing_failure:
                raise subprocess.CalledProcessError(1, command)
            dmg.with_suffix(".dmg.sha256").write_text("mock checksum")
        return subprocess.CompletedProcess(command, 0)

    def mock_mac(self):
        self.enter_context(patch.multiple(macos_sign, ROOT=self.root, CPP_BUILD_DIR=self.cpp))
        self.enter_context(patch.object(macos_sign.sys, "platform", "darwin"))
        self.enter_context(patch.object(macos_sign.sys.stdin, "isatty", return_value=True))
        self.enter_context(patch.object(macos_sign.platform, "machine", return_value="arm64"))
        self.enter_context(patch.object(macos_sign.shutil, "which", return_value="/opt/homebrew/bin/create-dmg"))
        self.enter_context(patch.object(macos_sign.subprocess, "run", side_effect=self.run_process))
        self.enter_context(patch("builtins.input", side_effect=["abc123def4", "example@example.com"]))
        return self.enter_context(patch.object(macos_sign.getpass, "getpass", side_effect=["export", "app-specific"]))

    def test_build_stage_and_sign_without_exposing_credentials_to_build_tools(self):
        passwords = self.mock_mac()
        with patch.dict(os.environ, {"APPLE_APP_PASSWORD": "inherited"}):
            self.run_cli()
        self.assertEqual(passwords.call_count, 2)
        processes = [argv for argv, _ in self.calls]
        self.assertEqual(processes[1], [str(self.root / "elaris"), "bundle"])
        self.assertEqual(self.calls[1][1]["ELARIS_BASELINE_CPU"], "1")
        for argv, env in self.calls[:-1]:
            self.assertFalse(any(name.startswith("APPLE_") for name in env), argv)
        argv, env = self.calls[-1]
        self.assertEqual(env["APPLE_CERTIFICATE"], base64.b64encode(b"mock p12").decode())
        self.assertEqual(env["APPLE_TEAM_ID"], "ABC123DEF4")
        self.assertEqual(env["APPLE_APP_PASSWORD"], "app-specific")
        self.assertEqual(env["APPLE_CERTIFICATE_PASSWORD"], "export")
        self.assertNotIn("app-specific", argv)
        self.assertNotIn("export", argv)
        self.assertEqual((self.app / "untouched").read_text(), "original")
        self.assertFalse(Path(argv[argv.index("--app") + 1]).exists())
        self.assertEqual(len(list(self.output.glob("*/Elaris-macos-arm64.dmg.sha256"))), 1)

    def test_skip_build_keeps_dmg_after_signer_failure(self):
        self.mock_mac()
        self.make_bundle()
        self.signing_failure = True
        with self.assertRaises(SystemExit) as error:
            self.run_cli("--skip-build")
        self.assertEqual(error.exception.code, 1)
        self.assertFalse(any(argv[-1] == "bundle" for argv, _ in self.calls))
        self.assertEqual(len(list(self.output.glob("*/Elaris-macos-arm64.dmg"))), 1)
        self.assertFalse(list(self.output.glob("*/*.sha256")))

    def test_missing_inputs_fail_before_prompting(self):
        passwords = self.mock_mac()
        with self.assertRaisesRegex(SystemExit, "Could not find the app"):
            self.run_cli("--skip-build")
        passwords.assert_not_called()
        self.assertFalse(self.output.exists())

    def test_password_echo_warning_aborts_before_signing(self):
        passwords = self.mock_mac()
        self.make_bundle()
        passwords.side_effect = lambda prompt: warnings.warn("Terminal echo on", getpass.GetPassWarning)
        with self.assertRaisesRegex(SystemExit, "Could not read credentials securely"):
            self.run_cli("--skip-build")
        self.assertFalse(self.output.exists())
        self.assertFalse(any("release" in argv for argv, _ in self.calls))

    def test_noninteractive_terminal_refuses_to_prompt(self):
        passwords = self.mock_mac()
        with patch.object(macos_sign.sys.stdin, "isatty", return_value=False):
            with self.assertRaisesRegex(SystemExit, "interactive terminal"):
                self.run_cli()
        passwords.assert_not_called()
        self.assertFalse(self.calls)
