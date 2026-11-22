"""Run the UI-only Elaris in a lightweight Linux desktop container, viewed in a browser."""

import argparse
import os
import platform

from .common import ROOT, require, run
from .linux import require_docker_daemon

DESKTOP_DIR = ROOT / "docker" / "desktop"
IMAGE = "elaris-desktop:latest"
CONTAINER = "elaris-desktop"
VIEWER_URL = "http://127.0.0.1:6080/vnc.html?autoconnect=1&resize=scale"


def desktop(args: argparse.Namespace) -> None:
    if platform.system() not in {"Darwin", "Linux"}:
        raise SystemExit("elaris: desktop is supported from macOS or Linux hosts")
    require("docker")
    require_docker_daemon()

    print("Building the desktop image. Docker output follows.", flush=True)
    run(
        "docker",
        "build",
        "--file",
        str(DESKTOP_DIR / "Dockerfile"),
        "--build-arg",
        f"USER_UID={os.getuid()}",
        "--build-arg",
        f"USER_GID={os.getgid()}",
        "--tag",
        IMAGE,
        str(DESKTOP_DIR),
    )

    print(f"Open {VIEWER_URL} in a browser once Elaris starts.", flush=True)
    command = [
        "docker",
        "run",
        "--rm",
        "--init",
        "--name",
        CONTAINER,
        "--publish",
        "127.0.0.1:6080:6080",
        "--mount",
        f"type=bind,source={ROOT},target=/workspace",
        # Named volumes keep Cargo's cache and build output away from the host's macOS target/ directory.
        "--mount",
        "type=volume,source=elaris-desktop-cargo,target=/home/elaris/.cargo",
        "--mount",
        "type=volume,source=elaris-desktop-target,target=/home/elaris/target",
        "--env",
        f"ELARIS_DESKTOP_SIZE={args.size}",
        IMAGE,
        "./elaris",
        "run",
        "--ui",
        *([args.url] if args.url else []),
    ]
    run(*command)


def register_commands(commands) -> None:
    command = commands.add_parser(
        "desktop",
        help="Run the UI-only Elaris full screen in a Linux container, shown in a browser",
    )
    command.add_argument("--size", default="1600x1000", help="Virtual screen size (default: 1600x1000)")
    command.add_argument("url", nargs="?", help="Address to open, such as example.com")
    command.set_defaults(handler=desktop)
