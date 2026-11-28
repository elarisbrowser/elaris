"""Check setup routing without installing packages or touching a real profile."""

from contextlib import redirect_stderr
import io
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import MagicMock, call, patch

from cli import parser
from elaris_cli import dependencies, project


class SetupTests(unittest.TestCase):
    @staticmethod
    def invoke(argv):
        args = parser().parse_args(["setup", *argv])
        args.handler(args)

    def test_ui_setup_and_launch_never_initialise_ladybird(self):
        with (
            patch.object(project, "dependencies") as prereq,
            patch.object(project, "run") as run,
            patch.object(project, "prepare_ladybird_paths") as prepare,
            patch.object(project, "launch") as launch,
        ):
            self.invoke(["--ui", "--run", "--jobs", "2"])
        self.assertEqual(prereq.mock_calls, [
            call.install_ui_system_dependencies(),
            call.verify_ui_system_dependencies(),
            call.install_rust_toolchain(),
        ])
        run.assert_not_called()
        prepare.assert_not_called()
        args = launch.call_args.args[0]
        self.assertTrue(args.ui)
        self.assertEqual(args.jobs, "2")
        self.assertIsNone(args.url)

    def test_full_build_keeps_the_pinned_dependency_setup_order(self):
        calls = MagicMock()
        with (
            patch.object(project, "dependencies", calls.dependencies),
            patch.object(project, "run", calls.run),
            patch.object(project, "prepare_ladybird_paths", calls.prepare),
            patch.object(project, "build", calls.build),
        ):
            self.invoke(["--skip-system-deps", "--build", "--jobs", "3"])
        self.assertEqual(calls.mock_calls[:-1], [
            call.dependencies.verify_system_dependencies(),
            call.dependencies.install_rust_toolchain(),
            call.run("git", "submodule", "update", "--init", "--recursive"),
            call.prepare(),
            call.dependencies.verify_ladybird_compiler(),
            call.dependencies.install_ladybird_toolchain(),
        ])
        calls.build.assert_called_once()
        args = calls.build.call_args.args[0]
        self.assertFalse(args.ui)
        self.assertEqual(args.jobs, "3")

    def test_build_and_run_are_mutually_exclusive(self):
        with redirect_stderr(io.StringIO()), self.assertRaises(SystemExit) as result:
            parser().parse_args(["setup", "--build", "--run"])
        self.assertEqual(result.exception.code, 2)

    def test_ui_setup_without_system_install_still_checks_prerequisites(self):
        with patch.object(project, "dependencies") as prereq:
            with patch.object(project, "launch") as launch:
                self.invoke(["--ui", "--skip-system-deps"])
        self.assertEqual(prereq.mock_calls, [
            call.verify_ui_system_dependencies(),
            call.install_rust_toolchain(),
        ])
        launch.assert_not_called()

    def test_apt_ui_install_does_not_add_package_repositories(self):
        with (
            patch.object(dependencies.platform, "system", return_value="Linux"),
            patch.object(dependencies, "_linux_package_manager", return_value="apt-get"),
            patch.object(dependencies, "_privileged_prefix", return_value=("sudo",)),
            patch.object(dependencies, "run") as run,
        ):
            dependencies.install_ui_system_dependencies()
        self.assertEqual(run.call_args_list[0], call("sudo", "apt-get", "update"))
        self.assertEqual(len(run.call_args_list), 2)
        self.assertEqual(run.call_args.args[:4], ("sudo", "apt-get", "install", "-y"))
        packages = run.call_args.args[4:]
        self.assertIn("libxkbcommon-dev", packages)
        self.assertIn("libwayland-dev", packages)
        self.assertFalse(any(name.startswith(("qt", "clang")) for name in packages))

    def test_rustup_uses_configured_cargo_home_without_changing_shell_profiles(self):
        with tempfile.TemporaryDirectory() as temporary:
            cargo_bin = Path(temporary) / "bin"
            cargo_bin.mkdir()
            with (
                patch.dict(os.environ, {"CARGO_HOME": temporary, "PATH": "/usr/bin"}),
                patch.object(dependencies, "command_path", return_value=None),
                patch.object(dependencies, "require"),
                patch.object(dependencies, "run") as run,
                patch.object(dependencies, "_ensure_rust_toolchain") as ensure,
            ):
                dependencies.install_rust_toolchain()
                self.assertEqual(os.environ["PATH"].split(os.pathsep)[0], str(cargo_bin))
            self.assertIn("--no-modify-path", run.call_args.args)
            ensure.assert_called_once_with("stable")

    def test_build_and_launch_use_the_same_custom_target_directory(self):
        with patch.dict(os.environ, {"CARGO_TARGET_DIR": "alternate-target"}):
            self.assertEqual(project._executable("debug").parent, project.ROOT / "alternate-target" / "debug")
            self.assertEqual(project._build_stamp(True, "debug").parent, project._executable("debug").parent)
