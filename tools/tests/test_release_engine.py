import argparse
import contextlib
import os
import plistlib
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from tools.elaris_cli import project, release_app, release_engine as native

KEY = "release-engine-v1-" + "b" * 64
PRODUCER = "c" * 40
ELF = b"\x7fELF\x02\x01" + bytes(12) + b"\x3e\x00"
MACH = b"\xcf\xfa\xed\xfe\x0c\x00\x00\x01" + bytes(12)


class ExactReleaseEngine(unittest.TestCase):
    def setUp(self):
        self.context = contextlib.ExitStack()
        self.addCleanup(self.context.close)
        self.root = Path(self.context.enter_context(tempfile.TemporaryDirectory()))
        for name, value in {
            "ROOT": self.root,
            "ENGINE": self.root / "vendor" / "ladybird",
            "CPP_BUILD_DIR": self.root / "build" / "cpp-ladybird",
            "VCPKG_INSTALLED": self.root / "deps",
            "CACHE": self.root / "cache",
            "BUILD_INFO": self.root / "build" / "release-engine-info.txt",
            "HSTS_FILE": self.root / "inputs" / "transport_security_state_static.json",
        }.items():
            self.context.enter_context(patch.object(native, name, value))
        self.context.enter_context(patch.object(native, "vcpkg_triplet", return_value="triplet"))
        self.context.enter_context(patch.object(native, "provenance", return_value={
            "producer": PRODUCER, "run": "123", "attempt": "2",
        }))
        native.HSTS_FILE.parent.mkdir()
        native.HSTS_FILE.write_bytes(b'{"entries": []}')

    def populate_native_outputs(self):
        roots = native._layout()
        for path, group in roots.items():
            path.mkdir(parents=True)
            if group == "resources":
                (path / "resource.dat").write_bytes(b"font and web resources")
        helpers = next(path for path, group in roots.items() if group == "helpers")
        arch = MACH if native.sys.platform == "darwin" else ELF
        for helper in native.HELPERS | ({"Ladybird", "ProcessReaper"} if arch == MACH else set()):
            path = helpers / helper
            path.write_bytes(arch)
            path.chmod(0o755)
            if arch == MACH and helper != "Ladybird":
                entitlement = native.CPP_BUILD_DIR / "Services" / helper / f"{helper}.entitlements"
                entitlement.parent.mkdir(parents=True)
                entitlement.write_bytes(plistlib.dumps({"com.apple.security.cs.disable-library-validation": True}))
        suffix = "dylib" if arch == MACH else "so"
        bridge = native.CPP_BUILD_DIR / "lib" / f"libelaris_ladybird.{suffix}"
        bridge.write_bytes(arch)
        dep = native.VCPKG_INSTALLED / "triplet" / "lib"
        version = f"libsupport.1.{suffix}" if arch == MACH else "libsupport.so.1"
        (dep / version).write_bytes(arch)
        (dep / f"libsupport.{suffix}").symlink_to(version)
        (dep / "development-only.a").write_bytes(b"not needed")

    def remove_native_outputs(self):
        shutil.rmtree(native.CPP_BUILD_DIR)
        shutil.rmtree(native.VCPKG_INSTALLED)

    def test_linux_and_mac_round_trip_without_cmake_state(self):
        for platform in ("linux", "darwin"):
            with self.subTest(platform=platform), patch.object(native.sys, "platform", platform):
                self.populate_native_outputs()
                (native.CPP_BUILD_DIR / "CMakeCache.txt").write_text("do not restore")
                exported = native.export(KEY)
                self.remove_native_outputs()
                self.assertEqual(native.restore(KEY), exported)
                self.assertFalse((native.CPP_BUILD_DIR / "CMakeCache.txt").exists())
                self.assertFalse((native.VCPKG_INSTALLED / "triplet/lib/development-only.a").exists())
                suffix = "dylib" if platform == "darwin" else "so"
                self.assertTrue((native.VCPKG_INSTALLED / "triplet/lib" / f"libsupport.{suffix}").is_symlink())
                self.remove_native_outputs()

    def test_missing_file_and_wrong_key_do_not_install_any_outputs(self):
        self.populate_native_outputs()
        native.export(KEY)
        self.remove_native_outputs()
        with self.assertRaisesRegex(ValueError, "does not match"):
            native.restore("release-engine-v1-" + "a" * 64)
        (native.CACHE / "helpers" / "WebContent").unlink()
        with self.assertRaisesRegex(ValueError, "incomplete or changed"):
            native.restore(KEY)
        self.assertFalse(native.CPP_BUILD_DIR.exists())
        self.assertFalse(native.VCPKG_INSTALLED.exists())

    def test_missing_mac_entitlement_cannot_be_exported(self):
        with patch.object(native.sys, "platform", "darwin"):
            self.populate_native_outputs()
            (native.CPP_BUILD_DIR / "Services" / "WebContent" / "WebContent.entitlements").unlink()
            with self.assertRaisesRegex(ValueError, "entitlements.*WebContent"):
                native.export(KEY)
            self.assertFalse((native.CACHE / native.MANIFEST).exists())

    def test_bundle_miss_hit_and_corrupt_hit(self):
        trace = []

        def build_engine():
            self.assertEqual(os.environ["ELARIS_RELEASE_HSTS_DIR"], str(native.HSTS_FILE.parent))
            self.populate_native_outputs()
            trace.append("native")

        def cargo(label, *args):
            self.assertEqual(args[-2:], ("--features", "ladybird-bridge"))
            trace.append("cargo")

        def bundle(_binary, _prefix, *, fresh):
            self.assertTrue(fresh)
            self.assertTrue((native.CPP_BUILD_DIR / "lib" / "libelaris_ladybird.so").is_file())
            trace.append("package")

        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(native.sys, "platform", "linux"))
            stack.enter_context(patch.object(native, "key", return_value=KEY))
            stack.enter_context(patch.object(project.engine, "build_for_elaris", side_effect=build_engine))
            stack.enter_context(patch.object(project, "run_step", side_effect=cargo))
            stack.enter_context(patch.object(project, "bridge_lib_dir", return_value="native/lib"))
            stack.enter_context(patch.object(project, "_configure_bridge_environment"))
            stack.enter_context(patch.object(project.packaging, "linux_prefix_bundle", side_effect=bundle))
            args = argparse.Namespace(release_engine_cache_key=KEY, release_engine_cache_hit=False, canary=False)

            project.bundle(args)
            self.assertEqual(trace, ["native", "cargo", "package"])
            self.assertIn("built natively", native.BUILD_INFO.read_text())
            self.remove_native_outputs()

            trace.clear()
            args.release_engine_cache_hit = True
            project.bundle(args)
            self.assertEqual(trace, ["cargo", "package"])
            self.assertIn("reused exact output", native.BUILD_INFO.read_text())
            self.assertIn(PRODUCER, native.BUILD_INFO.read_text())
            self.remove_native_outputs()

            trace.clear()
            (native.CACHE / "helpers" / "WebContent").write_bytes(b"broken")
            project.bundle(args)
            self.assertEqual(trace, ["native", "cargo", "package"])
            self.assertNotIn("reused", native.BUILD_INFO.read_text())

    def test_both_exact_hits_still_package_and_invalid_app_only_rebuilds_rust(self):
        trace = []
        executable = self.root / "target/release/elaris"
        app_key = "release-app-v1-" + "a" * 64
        args = argparse.Namespace(
            release_engine_cache_key=KEY, release_engine_cache_hit=False,
            release_app_cache_key=app_key, release_app_cache_hit=False, canary=False,
        )

        def build_engine():
            trace.append("native")
            self.populate_native_outputs()

        def cargo(*_args):
            trace.append("cargo")
            executable.parent.mkdir(parents=True, exist_ok=True)
            executable.write_bytes(ELF[:16] + b"\x03\x00" + ELF[18:] + b"elaris")
            executable.chmod(0o755)

        def package(binary, _prefix, *, fresh):
            trace.append(("package", binary, fresh))
            self.assertTrue((native.CPP_BUILD_DIR / "lib/libelaris_ladybird.so").is_file())

        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(native.sys, "platform", "linux"))
            stack.enter_context(patch.object(native, "key", return_value=KEY))
            stack.enter_context(patch.object(release_app, "key", return_value=app_key))
            stack.enter_context(patch.multiple(
                release_app, CACHE=self.root / "app-cache", CPP_BUILD_DIR=native.CPP_BUILD_DIR,
                BUILD_INFO=self.root / "app-info.txt",
            ))
            stack.enter_context(patch.object(project.engine, "build_for_elaris", side_effect=build_engine))
            stack.enter_context(patch.object(project, "_executable", return_value=executable))
            stack.enter_context(patch.object(project, "_build_stamp", return_value=self.root / "build/stamp"))
            stack.enter_context(patch.object(project, "run_step", side_effect=cargo))
            stack.enter_context(patch.object(project, "bridge_lib_dir", return_value="native/lib"))
            stack.enter_context(patch.object(project, "_configure_bridge_environment"))
            stack.enter_context(patch.object(project.packaging, "linux_prefix_bundle", side_effect=package))

            project.bundle(args)
            self.assertEqual(trace, ["native", "cargo", ("package", executable, True)])
            self.remove_native_outputs()

            trace.clear()
            args.release_engine_cache_hit = args.release_app_cache_hit = True
            project.bundle(args)
            self.assertEqual(trace, [("package", release_app.CACHE / "elaris", True)])
            self.remove_native_outputs()

            trace.clear()
            (release_app.CACHE / "elaris").write_bytes(b"corrupt")
            project.bundle(args)
            self.assertEqual(trace, ["cargo", ("package", executable, True)])
            self.assertIn("reused exact output", native.BUILD_INFO.read_text())

    def test_key_changes_for_native_input_not_rust_only_change(self):
        cargo = native.ROOT / "Cargo.toml"
        cargo.write_text('version = "0.1.0"\n')
        native.ENGINE.mkdir(parents=True)
        (native.ENGINE / "rust-toolchain.toml").write_text('channel = "1.98.0"\n')
        bridge = native.ROOT / "bridge.cpp"
        bridge.write_text("native source")
        browser = native.ROOT / "browser.rs"
        browser.write_text("Rust source")
        readme = native.ROOT / "bridge/README.md"
        readme.parent.mkdir()
        readme.write_text("Bridge documentation")
        signer = native.ROOT / "macos_sign.py"
        signer.write_text("Local signing")

        def output(*command, cwd=None):
            if command[:2] == ("git", "rev-parse"):
                return native.AUDITED_PIN
            if command[:2] == ("git", "status"):
                return ""
            if command[:2] == ("git", "ls-files"):
                paths = command[4:]
                self.assertIn("tools/elaris_cli/engine.py", paths)
                self.assertNotIn("tools/elaris_cli", paths)
                self.assertNotIn("tools/elaris_cli/macos_sign.py", paths)
                self.assertNotIn("tools/cli.py", paths)
                return "bridge.cpp\0bridge/README.md\0"
            return "toolchain identity"

        with patch.object(native.sys, "platform", "linux"), \
             patch.object(native.platform, "machine", return_value="x86_64"), \
             patch.object(native.shutil, "which", return_value=None), \
             patch.object(native, "_linux_abi", return_value=["Linux x86_64 fixture"]), \
             patch.object(native, "_output", side_effect=output), \
             patch.dict(os.environ, {"ELARIS_BASELINE_CPU": "1"}, clear=True):
            original = native.key()
            browser.write_text("changed Rust source")
            readme.write_text("Updated bridge documentation")
            signer.write_text("Updated signing command")
            self.assertEqual(native.key(), original)
            bridge.write_text("changed native source")
            new_bridge = native.key()
            self.assertNotEqual(new_bridge, original)
            cargo.write_text('version = "0.1.1"\n')
            new_version = native.key()
            self.assertNotEqual(new_version, new_bridge)
            native.HSTS_FILE.write_bytes(b'{"entries": [{"name": "test.example"}]}')
            self.assertNotEqual(native.key(), new_version)

    def test_live_preload_bytes_are_staged_before_the_key_and_replace_previous_data(self):
        data = b'// input\n{"entries": [{"name": "example.com"}]}'

        def curl(command, **_kwargs):
            Path(command[command.index("--output") + 1]).write_bytes(data)

        with patch.object(native, "_output", return_value=native.AUDITED_PIN), \
             patch.object(native.subprocess, "run", side_effect=curl), \
             patch.object(native, "key", side_effect=lambda: native._hash(native.HSTS_FILE)):
            first = native.prepare_key()
            self.assertEqual(native.HSTS_FILE.read_bytes(), data)
            data = b'{"entries": []}'
            self.assertNotEqual(native.prepare_key(), first)
            self.assertEqual(native.HSTS_FILE.read_bytes(), data)


if __name__ == "__main__":
    unittest.main()
