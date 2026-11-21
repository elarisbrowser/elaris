"""Pin Elaris to a commit of the Rune fork."""

import argparse
import re
import subprocess

from .common import ROOT, require, run

RUNE_GIT = "https://github.com/elarisbrowser/rune"
DEFAULT_BRANCH = "dev"
MANIFEST = ROOT / "Cargo.toml"
# A `git = <fork>` line and the `rev = "..."` line that follows it in the same table.
FORK_REV = re.compile(
    r'(git = "' + re.escape(RUNE_GIT) + r'"\n(?:[^\[\n]*\n)*?rev = ")([0-9a-f]{7,40})(")'
)
FULL_SHA = re.compile(r"[0-9a-f]{40}")


def resolve(ref: str) -> str:
    """Return the full commit a branch, tag or full SHA names on the fork."""
    if FULL_SHA.fullmatch(ref):
        return ref
    result = subprocess.run(
        ("git", "ls-remote", RUNE_GIT, f"refs/heads/{ref}", f"refs/tags/{ref}"),
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode:
        raise SystemExit(f"elaris: could not reach {RUNE_GIT}: {result.stderr.strip()}")
    line = result.stdout.split("\n", 1)[0]
    if not line:
        raise SystemExit(f"elaris: {ref!r} is not a branch, tag or full commit on {RUNE_GIT}")
    return line.split()[0]


def current_pins(manifest: str) -> set:
    return {match.group(2) for match in FORK_REV.finditer(manifest)}


def pin(args: argparse.Namespace) -> None:
    require("cargo", "git")
    override = ROOT / ".cargo" / "config.toml"
    if override.exists() and RUNE_GIT in override.read_text():
        raise SystemExit(f"elaris: {override} patches Rune locally; remove it before pinning")

    revision = resolve(args.ref)
    manifest = MANIFEST.read_text()
    pins = current_pins(manifest)
    if not pins:
        raise SystemExit("elaris: Cargo.toml has no elarisbrowser/rune dependencies to pin")
    if pins != {revision}:
        MANIFEST.write_text(FORK_REV.sub(lambda match: f"{match.group(1)}{revision}{match.group(3)}", manifest))
    # Resolving re-locks only the sources whose revision changed; every other package keeps its lock.
    quiet("cargo", "metadata", "--format-version", "1")
    quiet("cargo", "metadata", "--locked", "--no-deps", "--format-version", "1")
    print(f"elaris: Rune is pinned to {revision} (was {', '.join(sorted(pins))})")
    if not args.no_check:
        run("cargo", "+stable", "check", "--locked")


def quiet(*command: str) -> None:
    """Runs `command`, keeping its output unless it fails."""
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=False)
    if result.returncode:
        raise SystemExit(f"elaris: {' '.join(command)} failed:\n{result.stderr.strip()}")


def register_commands(commands) -> None:
    command = commands.add_parser("pin", help="Pin Rune and its patches to a fork commit")
    command.add_argument(
        "ref",
        nargs="?",
        default=DEFAULT_BRANCH,
        help=f"Fork branch, tag or full commit (default: the {DEFAULT_BRANCH} branch's head)",
    )
    command.add_argument("--no-check", action="store_true", help="Skip cargo check after pinning")
    command.set_defaults(handler=pin)
