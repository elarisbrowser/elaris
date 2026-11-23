"""Tests for the developer CLI's terminal output and build-step runner."""

import io
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from unittest import mock

from elaris_cli import ui
from elaris_cli.common import _PROGRESS, run_step


class FakeTerminal(io.StringIO):
    def __init__(self, tty: bool) -> None:
        super().__init__()
        self._tty = tty

    def isatty(self) -> bool:
        return self._tty


class ColourTests(unittest.TestCase):
    def setUp(self) -> None:
        ui.configure(no_color=False, verbose=False)
        self.env = mock.patch.dict(os.environ, {"TERM": "xterm-256color"}, clear=True)
        self.env.start()

    def tearDown(self) -> None:
        self.env.stop()
        ui.configure(no_color=False, verbose=False)

    def test_terminal_gets_theme_colour(self) -> None:
        self.assertEqual(ui.paint("ok", "success", FakeTerminal(True)), f"\x1b[{ui.THEME['success']}mok\x1b[0m")

    def test_pipes_get_plain_text(self) -> None:
        self.assertEqual(ui.paint("ok", "success", FakeTerminal(False)), "ok")

    def test_no_color_variable_wins_over_terminal(self) -> None:
        os.environ["NO_COLOR"] = "1"
        self.assertEqual(ui.paint("ok", "success", FakeTerminal(True)), "ok")

    def test_no_color_flag_wins_over_terminal(self) -> None:
        ui.configure(no_color=True)
        self.assertEqual(ui.paint("ok", "success", FakeTerminal(True)), "ok")

    def test_force_colour_applies_to_pipes(self) -> None:
        os.environ["FORCE_COLOR"] = "1"
        self.assertIn("\x1b[", ui.paint("ok", "success", FakeTerminal(False)))

    def test_dumb_terminal_gets_no_animation(self) -> None:
        os.environ["TERM"] = "dumb"
        with mock.patch.object(sys, "stdout", FakeTerminal(True)):
            self.assertFalse(ui.animated())


class FormatTests(unittest.TestCase):
    def test_durations(self) -> None:
        self.assertEqual(ui.format_duration(5.9), "5s")
        self.assertEqual(ui.format_duration(125), "2m 05s")

    def test_progress_pattern_reads_ninja_counts(self) -> None:
        match = _PROGRESS.search("[ 5/900] Building CXX object bridge/elaris.cpp.o")
        self.assertEqual((match[1], match[2]), ("5", "900"))

    def test_table_aligns_values(self) -> None:
        stream = io.StringIO()
        with redirect_stdout(stream):
            ui.table([("Short", "one", None), ("A longer label", "two", None)])
        lines = stream.getvalue().splitlines()
        self.assertEqual(lines[0].index("one"), lines[1].index("two"))


class RunStepTests(unittest.TestCase):
    def test_streamed_step_returns_on_success(self) -> None:
        with mock.patch.object(ui, "animated", return_value=False), redirect_stdout(io.StringIO()) as out:
            run_step("Streamed success", sys.executable, "-c", "print('hello')")
        self.assertIn("Streamed success", out.getvalue())

    def test_streamed_failure_keeps_the_exit_code(self) -> None:
        with mock.patch.object(ui, "animated", return_value=False), redirect_stdout(io.StringIO()):
            with self.assertRaises(SystemExit) as raised:
                run_step("Streamed failure", sys.executable, "-c", "import sys; sys.exit(3)")
        self.assertEqual(raised.exception.code, 3)

    def test_captured_step_writes_its_log_and_reports_progress(self) -> None:
        script = "print('[1/2] first'); print('[2/2] second')"
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(ui, "animated", return_value=True):
            with redirect_stdout(io.StringIO()):
                run_step("Captured step", sys.executable, "-c", script, log_dir=Path(directory))
            log = (Path(directory) / "captured-step.log").read_text()
        self.assertEqual(log.splitlines(), ["[1/2] first", "[2/2] second"])

    def test_captured_failure_keeps_the_log_and_exit_code(self) -> None:
        script = "print('compiler said no'); import sys; sys.exit(4)"
        with tempfile.TemporaryDirectory() as directory, mock.patch.object(ui, "animated", return_value=True):
            with redirect_stdout(io.StringIO()), mock.patch("sys.stderr", new_callable=io.StringIO) as err:
                with self.assertRaises(SystemExit) as raised:
                    run_step("Captured failure", sys.executable, "-c", script, log_dir=Path(directory))
            self.assertIn("compiler said no", err.getvalue())
            self.assertTrue((Path(directory) / "captured-failure.log").is_file())
        self.assertEqual(raised.exception.code, 4)


if __name__ == "__main__":
    unittest.main()
