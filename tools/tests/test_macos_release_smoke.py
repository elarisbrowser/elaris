"""Regression coverage for the packaged-browser smoke command."""

import os
import plistlib
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[2] / "packaging" / "macos" / "smoke_release.py"


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
                [sys.executable, SCRIPT, "--app", app],
                env={**os.environ, "APPLE_APP_PASSWORD": "mock-secret"},
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
