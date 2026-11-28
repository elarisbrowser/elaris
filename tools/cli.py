#!/usr/bin/env python3
"""Small developer command wrapper for Elaris and upstream Ladybird."""

import argparse
import sys

from elaris_cli import clean, cpp, desktop, engine, linux, macos_sign, patches, project, rune, ui


if sys.version_info < (3, 9):
    raise SystemExit("elaris: Python 3.9 or newer is required")

EXAMPLES = """\
examples:
  ./elaris setup --ui --run    set up and open the interface without Ladybird
  ./elaris setup --run         set up, build, and launch the full browser
  ./elaris run example.com     build if needed, then open a page
  ./elaris run --verbose       stream build and application logs
  ./elaris check               format, lint, and test
  ./elaris clean               remove build and target folders

Run './elaris <command> --help' for a command's options.
Global options go before the command, for example './elaris --verbose build'. The run command
also accepts --verbose after the command.
Set NO_COLOR=1 to turn off colour. Build logs are kept in build/logs/.
"""


def parser() -> argparse.ArgumentParser:
    cli = argparse.ArgumentParser(
        prog="./elaris",
        description="Developer commands for Elaris",
        epilog=EXAMPLES,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    cli.add_argument("--no-color", action="store_true", help="Print plain text without colour")
    cli.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Stream build output and, with run, application logs",
    )
    commands = cli.add_subparsers(dest="command", required=True)
    project.register_commands(commands)
    cpp.register_commands(commands)
    engine.register_commands(commands)
    patches.register_commands(commands)
    linux.register_commands(commands)
    macos_sign.register_commands(commands)
    desktop.register_commands(commands)
    rune.register_commands(commands)
    clean.register_commands(commands)
    return cli


def main() -> None:
    args = parser().parse_args()
    ui.configure(no_color=args.no_color, verbose=args.verbose)
    try:
        args.handler(args)
    except KeyboardInterrupt:
        ui.muted("Interrupted", stream=sys.stderr)
        raise SystemExit(130) from None
    except SystemExit as error:
        # Command failures pass a message; print it in the same style as every other error.
        if isinstance(error.code, str):
            ui.error(error.code.removeprefix("elaris: "))
            raise SystemExit(1) from None
        raise


if __name__ == "__main__":
    main()
