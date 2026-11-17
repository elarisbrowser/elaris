#!/usr/bin/env python3
"""Small developer command wrapper for Elaris and upstream Ladybird."""

import argparse

from elaris_cli import cpp, engine, project


def parser() -> argparse.ArgumentParser:
    cli = argparse.ArgumentParser(description="Developer commands for Elaris")
    commands = cli.add_subparsers(dest="command", required=True)
    project.register_commands(commands)
    cpp.register_commands(commands)
    engine.register_commands(commands)
    return cli


def main() -> None:
    args = parser().parse_args()
    args.handler(args)


if __name__ == "__main__":
    main()
