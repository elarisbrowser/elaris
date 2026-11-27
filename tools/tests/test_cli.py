"""Tests for the developer CLI argument parser."""

import io
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

from cli import parser
from elaris_cli.project import _run_with_live_log


class RunVerboseTests(unittest.TestCase):
    def test_verbose_after_run_is_enabled(self) -> None:
        args = parser().parse_args(["run", "--verbose"])
        self.assertTrue(args.verbose)

    def test_global_verbose_is_preserved(self) -> None:
        args = parser().parse_args(["--verbose", "run"])
        self.assertTrue(args.verbose)

    def test_verbose_after_run_preserves_url(self) -> None:
        args = parser().parse_args(["run", "--verbose", "example.com"])
        self.assertTrue(args.verbose)
        self.assertEqual(args.url, "example.com")

    def test_live_log_streams_all_lines(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            log_path = Path(directory) / "elaris-run.log"
            script = (
                "import sys, time\n"
                "from pathlib import Path\n"
                "path = Path(sys.argv[1])\n"
                "path.write_text('first\\n')\n"
                "time.sleep(0.1)\n"
                "with path.open('a') as log:\n"
                "    log.write('second\\n')\n"
            )
            output = io.StringIO()
            with redirect_stdout(output):
                _run_with_live_log(sys.executable, "-c", script, str(log_path), log_path=log_path)

        self.assertEqual(output.getvalue(), "first\nsecond\n")
