"""Regression coverage for the credential-gated macOS distribution pipeline."""

from __future__ import annotations

import base64
import hashlib
import importlib.util
import os
import plistlib
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


signing = _load("elaris_macos_signing", ROOT / "packaging" / "macos" / "signing.py")
artifact_guard = _load(
    "elaris_macos_artifact_guard", ROOT / "packaging" / "macos" / "verify_release_artifacts.py",
)


class SignedEntitlementTests(unittest.TestCase):
    def test_codesign_xml_stdout_is_separate_from_diagnostics(self):
        helper = Path("WebContent")
        expected = {"com.apple.security.cs.allow-jit": True}
        result = subprocess.CompletedProcess([], 0, plistlib.dumps(expected).decode(), 'Executable=/tmp/<WebContent>')
        with patch.object(signing, "_run", return_value=result) as run:
            self.assertEqual(signing._signed_entitlements(helper), expected)
        run.assert_called_once_with(
            ["codesign", "--display", "--entitlements", "-", "--xml", helper], check=False,
        )

    def test_bad_output_fails_closed_with_a_signing_error(self):
        valid = plistlib.dumps({"com.apple.security.cs.allow-jit": True}).decode()
        for code, output in ((1, valid), (0, ""), (0, '<?xml version="1.0"?>\n<plist><&>'),
                             (0, plistlib.dumps(["not a dictionary"]).decode())):
            with self.subTest(code=code, output=output):
                with patch.object(signing, "_run", return_value=subprocess.CompletedProcess([], code, output, "")):
                    with self.assertRaisesRegex(signing.SigningError, "Could not read signed entitlements"):
                        signing._signed_entitlements(Path("WebContent"))

    @unittest.skipUnless(sys.platform == "darwin", "requires macOS codesign")
    def test_reads_entitlements_from_a_real_ad_hoc_signature(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            helper = root / "WebContent"
            entitlements = root / "WebContent.entitlements"
            expected = {"com.apple.security.cs.allow-jit": True}
            entitlements.write_bytes(plistlib.dumps(expected))
            subprocess.run(
                ["xcrun", "clang", "-x", "c", "-", "-o", helper],
                input="int main(void) { return 0; }", text=True, check=True,
            )
            subprocess.run(
                ["codesign", "--force", "--sign", "-", "--options", "runtime",
                 "--entitlements", entitlements, helper],
                check=True, capture_output=True,
            )
            self.assertEqual(signing._signed_entitlements(helper), expected)


class PackageSmokeTests(unittest.TestCase):
    def test_smoke_cli_does_not_need_credentials_or_wait_for_the_app_to_exit(self):
        with tempfile.TemporaryDirectory() as directory:
            app = Path(directory) / "Elaris.app"
            executable = app / "Contents" / "MacOS" / "elaris"
            executable.parent.mkdir(parents=True)
            (app / "Contents" / "Info.plist").write_bytes(plistlib.dumps({"CFBundleExecutable": "elaris"}))
            executable.write_text(
                "#!/usr/bin/env python3\n"
                "import os, pathlib, sys, time\n"
                "assert 'APPLE_APP_PASSWORD' not in os.environ\n"
                "assert sys.argv[2:4] == ['--', '--snap']\n"
                "pathlib.Path(sys.argv[4]).write_bytes(b'fake-render')\n"
                "time.sleep(30)\n"
            )
            executable.chmod(0o755)
            result = subprocess.run(
                [sys.executable, signing.__file__, "smoke", "--app", app],
                env={**os.environ, "APPLE_APP_PASSWORD": "mock-secret"},
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)


class SigningPlanTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.app = self.root / "Elaris.app"
        self.macos = self.app / "Contents" / "MacOS"
        self.macos.mkdir(parents=True)
        self.entitlements = self.root / "entitlements"
        self.entitlements.mkdir()
        (self.app / "Contents" / "Info.plist").write_bytes(
            plistlib.dumps({"CFBundleExecutable": "elaris", "CFBundleIdentifier": "org.elaris.browser"})
        )
        self.macho(self.macos / "elaris")
        self.macho(self.macos / "WebContent")
        self.macho(self.macos / "ProcessReaper")
        self.macho(self.app / "Contents" / "lib" / "libengine.dylib")
        self.helper_entitlement("WebContent", {"com.apple.security.cs.allow-jit": True})
        self.helper_entitlement("ProcessReaper", {})

        framework = self.app / "Contents" / "Frameworks" / "Engine.framework"
        (framework / "Versions" / "A").mkdir(parents=True)
        (framework / "Resources").mkdir()
        (framework / "Resources" / "Info.plist").write_bytes(
            plistlib.dumps({"CFBundleExecutable": "Engine"})
        )
        self.macho(framework / "Versions" / "A" / "Engine")

        xpc = self.app / "Contents" / "XPCServices" / "Worker.xpc"
        (xpc / "Contents" / "MacOS").mkdir(parents=True)
        (xpc / "Contents" / "Info.plist").write_bytes(
            plistlib.dumps({"CFBundleExecutable": "Worker"})
        )
        self.macho(xpc / "Contents" / "MacOS" / "Worker")

        nested_framework = xpc / "Contents" / "Frameworks" / "Inner.framework"
        (nested_framework / "Versions" / "A").mkdir(parents=True)
        (nested_framework / "Resources").mkdir()
        (nested_framework / "Resources" / "Info.plist").write_bytes(
            plistlib.dumps({"CFBundleExecutable": "Inner"})
        )
        self.macho(nested_framework / "Versions" / "A" / "Inner")

    def tearDown(self):
        self.temporary.cleanup()

    @staticmethod
    def macho(path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fake Mach-O fixture")

    def helper_entitlement(self, name: str, values: dict):
        (self.entitlements / f"{name}.entitlements").write_bytes(plistlib.dumps(values))

    def fake_file(self, command, **kwargs):
        descriptions = []
        for value in command[2:]:
            path = Path(value)
            if path.read_bytes().startswith(b"fake Mach-O"):
                descriptions.append(f"Mach-O 64-bit executable {path.name} x86_64")
            else:
                descriptions.append("ASCII text")
        return subprocess.CompletedProcess(command, 0, "\n".join(descriptions), "")

    def test_nested_executables_are_discovered_without_architecture_assumptions(self):
        with patch.object(signing, "_run", side_effect=self.fake_file):
            discovered = signing.discover_macho_files(self.app)
        paths = {path.relative_to(self.app).as_posix() for path in discovered}
        self.assertEqual(len(paths), 7)
        self.assertIn("Contents/MacOS/elaris", paths)
        self.assertIn("Contents/Frameworks/Engine.framework/Versions/A/Engine", paths)
        self.assertIn("Contents/XPCServices/Worker.xpc/Contents/MacOS/Worker", paths)

    def test_signing_plan_orders_libraries_helpers_nested_bundles_and_app(self):
        with patch.object(signing, "_run", side_effect=self.fake_file):
            plan = signing.signing_plan(self.app, self.entitlements)
        targets = [target.path for target in plan]
        self.assertEqual(targets[-1], self.app.resolve())
        self.assertLess(
            targets.index((self.app / "Contents" / "lib" / "libengine.dylib").resolve()),
            targets.index((self.macos / "WebContent").resolve()),
        )
        self.assertLess(
            targets.index((self.app / "Contents" / "Frameworks" / "Engine.framework").resolve()),
            targets.index(self.app.resolve()),
        )
        self.assertLess(
            targets.index((self.app / "Contents" / "XPCServices" / "Worker.xpc").resolve()),
            targets.index(self.app.resolve()),
        )
        inner_framework = (
            self.app / "Contents" / "XPCServices" / "Worker.xpc"
            / "Contents" / "Frameworks" / "Inner.framework"
        )
        xpc = self.app / "Contents" / "XPCServices" / "Worker.xpc"
        self.assertLess(targets.index(inner_framework.resolve()), targets.index(xpc.resolve()))
        self.assertNotIn((self.macos / "elaris").resolve(), targets)
        web_content = next(target for target in plan if target.path == (self.macos / "WebContent").resolve())
        self.assertEqual(web_content.entitlements, self.entitlements / "WebContent.entitlements")

    def test_framework_rpath_dependencies_resolve_from_loader_path(self):
        framework_binary = self.app / "Contents" / "Frameworks" / "Engine.framework" / "Versions" / "A" / "Engine"
        support_library = framework_binary.parent / "libsupport.dylib"
        self.macho(support_library)
        discovered = [
            path.resolve()
            for path in self.app.rglob("*")
            if path.is_file() and path.read_bytes().startswith(b"fake Mach-O")
        ]

        def fake_otool(command, **kwargs):
            command = [str(part) for part in command]
            binary = Path(command[-1])
            if command[1:3] == ["-L", str(framework_binary)]:
                output = f"{binary}:\n\t@rpath/libsupport.dylib (compatibility version 1.0.0, current version 1.0.0)\n"
            elif command[1:3] == ["-l", str(framework_binary)]:
                output = (
                    "Load command 1\n          cmd LC_RPATH\n      cmdsize 32\n"
                    "         path @loader_path (offset 12)\n"
                )
            elif command[1] == "-L":
                output = f"{binary}:\n"
            else:
                output = ""
            return subprocess.CompletedProcess(command, 0, output, "")

        with patch.object(signing, "_run", side_effect=fake_otool):
            signing._verify_dependencies(self.app.resolve(), discovered)

    def test_missing_or_debug_entitlements_fail_closed(self):
        with patch.object(signing, "_run", side_effect=self.fake_file):
            (self.entitlements / "WebContent.entitlements").unlink()
            with self.assertRaisesRegex(signing.SigningError, "Missing release entitlements"):
                signing.signing_plan(self.app, self.entitlements)

            self.helper_entitlement("WebContent", {
                "com.apple.security.cs.allow-jit": True,
                "com.apple.security.get-task-allow": True,
            })
            with self.assertRaisesRegex(signing.SigningError, "Unsafe or unaudited"):
                signing.signing_plan(self.app, self.entitlements)


class CredentialAndSigningTests(unittest.TestCase):
    def credentials(self):
        return signing.Credentials(
            certificate=b"test p12 bytes",
            certificate_password="certificate-password",
            team_id="A1B2C3D4E5",
            apple_id="developer@example.test",
            app_password="app-specific-password",
        )

    def test_missing_credentials_are_explicit(self):
        with self.assertRaisesRegex(signing.SigningError, "APPLE_CERTIFICATE.*APPLE_TEAM_ID"):
            signing.load_credentials({})

    def test_signing_secrets_are_removed_from_child_process_environments(self):
        values = {name: "should-not-propagate" for name in signing.REQUIRED_CREDENTIALS}
        with patch.dict("os.environ", values, clear=True), patch.object(
            signing.subprocess, "run", return_value=subprocess.CompletedProcess(["tool"], 0, "", ""),
        ) as run:
            signing._run(["tool"])
        child_environment = run.call_args.kwargs["env"]
        self.assertFalse(set(signing.REQUIRED_CREDENTIALS) & child_environment.keys())

    def test_command_timeout_does_not_log_notarization_password_arguments(self):
        secret = "private-app-password"
        with patch.object(
            signing.subprocess, "run", side_effect=subprocess.TimeoutExpired(["notarytool", "--password", secret], 3),
        ):
            with self.assertRaises(signing.SigningError) as context:
                signing._run(["notarytool", "--password", secret], timeout=3)
        self.assertNotIn(secret, str(context.exception))

    def test_temporary_keychain_is_removed_when_import_fails(self):
        calls = []

        def run(command, **kwargs):
            command = [str(value) for value in command]
            calls.append(command)
            if command[:3] == ["security", "list-keychains", "-d"] and "-s" not in command:
                return subprocess.CompletedProcess(command, 0, '    "/Users/test/login.keychain-db"\n', "")
            if command[:2] == ["security", "import"]:
                raise signing.SigningError("simulated import failure")
            return subprocess.CompletedProcess(command, 0, "", "")

        keychain = signing.TemporaryKeychain(self.credentials())
        with patch.object(signing, "_run", side_effect=run):
            with self.assertRaisesRegex(signing.SigningError, "simulated import failure"):
                with keychain:
                    self.fail("keychain import should have failed")
        self.assertFalse(keychain.directory.exists())
        self.assertTrue(any(command[:2] == ["security", "delete-keychain"] for command in calls))
        self.assertFalse(any("test p12 bytes" in " ".join(command) for command in calls))

    def test_signing_failure_stops_before_packaging_and_runs_cleanup(self):
        target = signing.SigningTarget(Path("WebContent"))

        class Keychain:
            cleaned = False

            def __enter__(self):
                return Path("temporary.keychain-db")

            def __exit__(self, *_args):
                self.cleaned = True

        keychain = Keychain()
        values = {
            "APPLE_CERTIFICATE": base64.b64encode(b"p12").decode(),
            "APPLE_CERTIFICATE_PASSWORD": "password",
            "APPLE_TEAM_ID": "A1B2C3D4E5",
            "APPLE_ID": "developer@example.test",
            "APPLE_APP_PASSWORD": "app-password",
        }
        with patch.dict("os.environ", values, clear=True), \
             patch.object(signing, "signing_plan", return_value=[target]), \
             patch.object(signing, "TemporaryKeychain", return_value=keychain), \
             patch.object(signing, "_identity_for_team", return_value="A" * 40), \
             patch.object(signing, "_sign_target", side_effect=signing.SigningError("simulated codesign failure")), \
             patch.object(signing, "_run") as run:
            with tempfile.TemporaryDirectory() as directory:
                with self.assertRaisesRegex(signing.SigningError, "simulated codesign failure"):
                    signing.release(
                        Path("Elaris.app"), Path("entitlements"), Path(directory) / "Elaris.dmg", "Elaris",
                    )
            run.assert_not_called()
        self.assertTrue(keychain.cleaned)

    def test_notarization_rejection_retrieves_apple_diagnostics(self):
        credentials = self.credentials()
        submit = subprocess.CompletedProcess(
            ["xcrun"], 65, '{"id":"submission-123","status":"Rejected"}', "",
        )
        log = subprocess.CompletedProcess(["xcrun"], 0, '{"issues":[{"message":"invalid signature"}]}', "")
        with patch.object(signing, "_run", side_effect=[submit, log]) as run:
            with self.assertRaisesRegex(signing.SigningError, "notarization was not accepted"):
                signing._notarize(Path("Elaris.dmg"), credentials)
        self.assertIn("notarytool", run.call_args_list[1].args[0])
        self.assertIn("submission-123", run.call_args_list[1].args[0])


class ArtifactPublicationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.dmg = self.directory / "Elaris-0.1.0-macos-arm64.dmg"
        self.dmg.write_bytes(b"signed and stapled disk image")
        checksum = hashlib.sha256(self.dmg.read_bytes()).hexdigest()
        (self.directory / f"{self.dmg.name}.sha256").write_text(f"{checksum}  {self.dmg.name}\n")
        (self.directory / f"{self.dmg.stem}.BUILD-INFO.txt").write_text("Elaris 0.1.0\n")
        (self.directory / "SIGNING-INFO.txt").write_text(
            "Status: notarized\n"
            "Team ID: A1B2C3D4E5\n"
            "Identity type: Developer ID Application\n"
            "Hardened Runtime: enabled for all Mach-O code\n"
            "Notarization: accepted\n"
            "Submission ID: submission-123\n"
            "Stapling: validated\n"
        )

    def tearDown(self):
        self.temporary.cleanup()

    def test_complete_signed_package_passes_publication_guard(self):
        self.assertEqual(artifact_guard.verify_release_artifacts(self.directory), self.dmg)

    def test_missing_or_incomplete_signing_metadata_blocks_publication(self):
        (self.directory / "SIGNING-INFO.txt").unlink()
        with self.assertRaisesRegex(artifact_guard.ArtifactError, "SIGNING-INFO"):
            artifact_guard.verify_release_artifacts(self.directory)

    def test_modified_package_blocks_publication(self):
        self.dmg.write_bytes(b"modified after checksum")
        with self.assertRaisesRegex(artifact_guard.ArtifactError, "checksum does not match"):
            artifact_guard.verify_release_artifacts(self.directory)


if __name__ == "__main__":
    unittest.main()
