"""Release executable cache validation and routing tests without Ladybird or network."""

import contextlib
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.elaris_cli import project, release_app, release_engine

ENGINE_KEY = "release-engine-v1-" + "a" * 64
APP_KEY = "release-app-v1-" + "b" * 64
ELF = b"\x7fELF\x02\x01" + bytes(10) + b"\x03\x00\x3e\x00"
MACH = b"\xcf\xfa\xed\xfe\x0c\x00\x00\x01" + bytes(4) + b"\x02\x00\x00\x00" + bytes(4)


class ExactReleaseApp(unittest.TestCase):
    def setUp(self):
        stack = contextlib.ExitStack()
        self.addCleanup(stack.close)
        self.root = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        self.cargo = self.root / "cargo"
        self.cargo.mkdir()
        self.binary = self.root / "target/release/elaris"
        self.binary.parent.mkdir(parents=True)
        self.libs = self.root / "cpp/lib"
        self.libs.mkdir(parents=True)
        stack.enter_context(patch.multiple(
            release_app, CACHE=self.root / "app-cache",
            CPP_BUILD_DIR=self.libs.parent, BUILD_INFO=self.root / "info.txt",
        ))
        stack.enter_context(patch.dict(os.environ, {"CARGO_HOME": str(self.cargo)}, clear=True))
        stack.enter_context(patch.object(release_app.sys, "platform", "linux"))
        self.create_binary(ELF)
        self.bridge = self.libs / "libelaris_ladybird.so"
        self.bridge.write_bytes(b"native bridge")

    def create_binary(self, header):
        self.binary.write_bytes(header + b"elaris")
        self.binary.chmod(0o755)

    def test_key_includes_embedded_revision_channel_toolchain_and_cargo_config(self):
        commit = "c" * 40
        rust = "rustc version info"

        def output(*argv):
            if argv[:2] == ("git", "rev-parse"):
                return commit
            if argv[:2] == ("git", "status"):
                return ""
            return rust if argv[0] == "rustc" else "cargo version"

        with patch.object(release_engine, "_output", side_effect=output):
            initial = release_app.key(ENGINE_KEY, "alpha")
            self.assertEqual(initial, release_app.key(ENGINE_KEY, "alpha"))
            self.assertNotEqual(initial, release_app.key(ENGINE_KEY, "canary"))
            self.assertNotEqual(initial, release_app.key(ENGINE_KEY[:-1] + "0", "alpha"))
            commit = "d" * 40
            self.assertNotEqual(initial, release_app.key(ENGINE_KEY, "alpha"))
            commit = "c" * 40
            rust += " updated"
            self.assertNotEqual(initial, release_app.key(ENGINE_KEY, "alpha"))
            rust = "rustc version info"
            (self.cargo / "config.toml").write_text("[build]\njobs = 2\n")
            self.assertNotEqual(initial, release_app.key(ENGINE_KEY, "alpha"))

    def test_dirty_checkout_cannot_use_cache(self):
        with patch.object(release_engine, "_output", side_effect=["c" * 40, " M src/main.rs"]):
            with self.assertRaisesRegex(ValueError, "clean checkout"):
                release_app.key(ENGINE_KEY, "alpha")

    def test_validates_architecture_bytes_mode_and_bridge_on_both_platforms(self):
        for host, header, suffix, wrong_header in (
            ("linux", ELF, "so", MACH), ("darwin", MACH, "dylib", ELF),
        ):
            with self.subTest(host=host), patch.object(release_app.sys, "platform", host):
                bridge = self.libs / f"libelaris_ladybird.{suffix}"
                bridge.write_bytes(b"original bridge")
                self.create_binary(header)
                manifest = release_app.export(self.binary, APP_KEY)
                cached, restored = release_app.restore(APP_KEY)
                self.assertEqual(manifest, restored)
                self.assertEqual(cached.read_bytes(), self.binary.read_bytes())
                with self.assertRaisesRegex(ValueError, "does not match"):
                    release_app.restore(APP_KEY[:-1] + "0")
                bridge.write_bytes(b"new bridge")
                with self.assertRaisesRegex(ValueError, "does not match"):
                    release_app.restore(APP_KEY)
                bridge.write_bytes(b"original bridge")
                cached.chmod(0o644)
                with self.assertRaisesRegex(ValueError, "not a supported executable"):
                    release_app.restore(APP_KEY)
                cached.chmod(0o755)
                cached.write_bytes(header + b"modified")
                with self.assertRaisesRegex(ValueError, "does not match"):
                    release_app.restore(APP_KEY)
                cached.unlink()
                cached.symlink_to(self.binary)
                with self.assertRaisesRegex(ValueError, "not a supported executable"):
                    release_app.restore(APP_KEY)
                self.create_binary(wrong_header)
                with self.assertRaisesRegex(ValueError, "not a supported executable"):
                    release_app.export(self.binary, APP_KEY)

    def test_uses_cargo_if_engine_is_cold_or_either_cache_is_invalid(self):
        release_app.export(self.binary, APP_KEY)
        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(project, "_executable", return_value=self.binary))
            stack.enter_context(patch.object(project, "_configure_bridge_environment"))
            stack.enter_context(patch.object(project, "bridge_lib_dir", return_value=str(self.libs)))
            cargo = stack.enter_context(patch.object(project, "run_step"))
            get_key = stack.enter_context(patch.object(release_app, "key", return_value=APP_KEY))
            cached, _ = release_app.restore(APP_KEY)
            self.assertEqual(project._release_executable(ENGINE_KEY, True, "alpha", APP_KEY, True), cached)
            cargo.assert_not_called()
            self.assertIn("reused exact output", release_app.BUILD_INFO.read_text())

            self.assertEqual(project._release_executable(ENGINE_KEY, False, "alpha", APP_KEY, True), self.binary)
            cargo.assert_called_once()
            self.assertIn("built with Cargo", release_app.BUILD_INFO.read_text())

            cached.write_bytes(b"bad archive")
            self.assertEqual(project._release_executable(ENGINE_KEY, True, "alpha", APP_KEY, True), self.binary)
            self.assertEqual(cargo.call_count, 2)

            get_key.side_effect = subprocess.CalledProcessError(1, ["rustc"])
            self.assertEqual(project._release_executable(ENGINE_KEY, True, "alpha", APP_KEY, True), self.binary)
            self.assertEqual(cargo.call_count, 3)
