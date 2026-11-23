"""Terminal output for the Elaris developer CLI: colour, status lines, spinners, and progress bars.

Colours and symbols are defined once here. Output is plain text when stdout is not a terminal,
when NO_COLOR is set, or when --no-color is given. Animation runs only on an interactive terminal
without --verbose, so CI logs and pipes stay readable.
"""

from __future__ import annotations

import os
import shutil
import sys
import threading
import time
from typing import Optional, Sequence, TextIO

# Each role names an ANSI SGR code. Change a colour here and every message follows.
THEME = {
    "success": "32",
    "warning": "33",
    "error": "1;31",
    "accent": "36",
    "heading": "1",
    "muted": "2",
}

SYMBOLS = {"ok": "✓", "failed": "✗", "warning": "!", "step": "→", "fill": "█", "empty": "░"}
ASCII_SYMBOLS = {"ok": "+", "failed": "x", "warning": "!", "step": ">", "fill": "#", "empty": "-"}
SPINNER_FRAMES = "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
ASCII_SPINNER_FRAMES = "|/-\\"
BAR_WIDTH = 20
FRAME_INTERVAL = 0.08
HIDE_CURSOR = "\x1b[?25l"
SHOW_CURSOR = "\x1b[?25h"
CLEAR_LINE = "\r\x1b[2K"

# Outcome of a finished step: (symbol, role, note appended to the title).
OUTCOMES = {
    "ok": ("ok", "success", ""),
    "failed": ("failed", "error", "failed"),
    "interrupted": ("failed", "warning", "interrupted"),
}

_no_color = False
_verbose = False


def configure(*, no_color: bool = False, verbose: bool = False) -> None:
    """Apply the global --no-color and --verbose options."""
    global _no_color, _verbose
    _no_color = no_color
    _verbose = verbose


def _is_terminal(stream: TextIO) -> bool:
    try:
        return stream.isatty() and os.environ.get("TERM") != "dumb"
    except (AttributeError, ValueError):
        return False


def _unicode_ok(stream: TextIO) -> bool:
    return (getattr(stream, "encoding", None) or "").lower().startswith("utf")


def _symbol(name: str, stream: TextIO) -> str:
    return (SYMBOLS if _unicode_ok(stream) else ASCII_SYMBOLS)[name]


def colour_enabled(stream: Optional[TextIO] = None) -> bool:
    if _no_color or os.environ.get("NO_COLOR"):
        return False
    if os.environ.get("FORCE_COLOR", "0") not in ("", "0"):
        return True
    return _is_terminal(stream or sys.stdout)


def animated() -> bool:
    """Whether spinners and progress bars can redraw a line in place."""
    return not _verbose and _is_terminal(sys.stdout)


def paint(text: str, role: str, stream: Optional[TextIO] = None) -> str:
    if not colour_enabled(stream):
        return text
    return f"\x1b[{THEME[role]}m{text}\x1b[0m"


def format_duration(seconds: float) -> str:
    total = int(seconds)
    minutes, secs = divmod(total, 60)
    return f"{minutes}m {secs:02d}s" if minutes else f"{secs}s"


def _prefix(position: Optional[tuple[int, int]]) -> str:
    return f"[{position[0]}/{position[1]}] " if position else ""


def heading(text: str) -> None:
    print(paint(text, "heading"), flush=True)


def muted(text: str, stream: Optional[TextIO] = None) -> None:
    print(paint(text, "muted", stream), file=stream or sys.stdout, flush=True)


def success(text: str) -> None:
    print(f"{paint(_symbol('ok', sys.stdout), 'success')} {text}", flush=True)


def warning(text: str) -> None:
    print(f"{paint(_symbol('warning', sys.stdout), 'warning')} {text}", flush=True)


def info(text: str) -> None:
    print(f"{paint(_symbol('step', sys.stdout), 'accent')} {text}", flush=True)


def error(message: str) -> None:
    stream = sys.stderr
    print(f"{paint('error', 'error', stream)}: {message}", file=stream, flush=True)


def table(rows: Sequence[tuple[str, str, Optional[str]]], *, label_role: Optional[str] = None) -> None:
    """Print aligned label and value rows. Each row's optional role colours its value."""
    width = max(len(label) for label, _, _ in rows)
    for label, value, role in rows:
        shown = paint(value, role) if role else value
        label_shown = paint(label.ljust(width), label_role) if label_role else label.ljust(width)
        print(f"  {label_shown}  {shown}")


def next_steps(steps: Sequence[tuple[str, str]]) -> None:
    """Suggest the commands that usually come next, as a short table."""
    print()
    heading("Next steps")
    table([(command, description, "muted") for command, description in steps], label_role="accent")


def announce(title: str, position: Optional[tuple[int, int]] = None) -> None:
    print(f"{paint(_symbol('step', sys.stdout), 'accent')} {_prefix(position)}{title}", flush=True)


def step_result(title: str, position: Optional[tuple[int, int]], seconds: float, outcome: str = "ok") -> None:
    symbol, role, note = OUTCOMES[outcome]
    suffix = f" {note}" if note else ""
    clock = paint(format_duration(seconds), "muted")
    print(f"{paint(_symbol(symbol, sys.stdout), role)} {_prefix(position)}{title}{suffix}  {clock}", flush=True)


class Progress:
    """A spinner, elapsed time, and optional progress bar redrawn on one terminal line."""

    def __init__(self, title: str, position: Optional[tuple[int, int]] = None) -> None:
        self._label = f"{_prefix(position)}{title}"
        self._started = time.monotonic()
        self._detail = ""
        self._fraction: Optional[tuple[int, int]] = None
        self._stop = threading.Event()
        self._thread = threading.Thread(target=self._animate, daemon=True)

    def start(self) -> None:
        sys.stdout.write(HIDE_CURSOR)
        sys.stdout.flush()
        self._thread.start()

    def update(self, detail: str, fraction: Optional[tuple[int, int]] = None) -> None:
        """Set the latest output line and, when the tool reports one, its (done, total) count."""
        self._detail = detail
        if fraction is not None:
            self._fraction = fraction

    def stop(self) -> None:
        """Clear the line and restore the cursor. Safe to call more than once."""
        self._stop.set()
        if self._thread.ident is not None:
            self._thread.join()
        if sys.stdout.isatty():
            sys.stdout.write(CLEAR_LINE + SHOW_CURSOR)
            sys.stdout.flush()

    def _animate(self) -> None:
        while not self._stop.is_set():
            self._draw()
            self._stop.wait(FRAME_INTERVAL)

    def _draw(self) -> None:
        stream = sys.stdout
        elapsed = time.monotonic() - self._started
        frames = SPINNER_FRAMES if _unicode_ok(stream) else ASCII_SPINNER_FRAMES
        spinner = frames[int(elapsed / FRAME_INTERVAL) % len(frames)]

        meter = meter_plain = ""
        if self._fraction and self._fraction[1]:
            done, total = self._fraction
            ratio = min(done / total, 1.0)
            filled = round(ratio * BAR_WIDTH)
            bar = _symbol("fill", stream) * filled + _symbol("empty", stream) * (BAR_WIDTH - filled)
            percent = f"{round(ratio * 100):>3}%"
            meter_plain = f"  {bar} {percent}"
            meter = f"  {paint(bar, 'accent')} {percent}"

        clock = format_duration(elapsed)
        columns = shutil.get_terminal_size((80, 24)).columns
        room = columns - len(spinner) - len(self._label) - len(meter_plain) - len(clock) - 6
        detail = " ".join(self._detail.split())[: max(room, 0)]
        tail = f"  {paint(clock, 'muted')}" + (f"  {paint(detail, 'muted')}" if detail else "")
        stream.write(f"{CLEAR_LINE}{paint(spinner, 'accent')} {self._label}{meter}{tail}")
        stream.flush()
