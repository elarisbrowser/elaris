"""Build, test and display Elaris for Linux in the devcontainer image, for either x86_64 or arm64."""

from __future__ import annotations

import argparse
import hashlib
import os
import platform
import socket
import subprocess
from pathlib import Path

from . import engine, ui
from .common import ENGINE, ROOT, require, run

DEVCONTAINER_DIR = ROOT / ".devcontainer"
DOCKERFILE = DEVCONTAINER_DIR / "Dockerfile"
PREPARE_VOLUMES = DEVCONTAINER_DIR / "prepare-volumes.sh"
DISPLAY_ENTRYPOINT = "/usr/local/bin/elaris-desktop-entrypoint"
NOVNC_PORT = 6080
PLATFORMS = {"amd64": "linux/amd64", "arm64": "linux/arm64"}


def linux_build(args: argparse.Namespace) -> None:
    """Set up Ladybird and build Elaris and its bridge, without running any tests."""
    _run_in_container(
        args,
        ["/bin/sh", "-c", './elaris setup --skip-system-deps && ./elaris build --jobs "$ELARIS_JOBS"'],
    )


def linux_test(args: argparse.Namespace) -> None:
    """Run the whole Linux suite: build, bridge tests, Rust checks, the release bundle, and the bundled browser."""
    # An x86_64 container on an arm64 host runs under emulation, where the sandbox's file broker cannot start.
    emulated = (args.arch or _host_architecture()) == "amd64" and _host_architecture() == "arm64"
    _run_in_container(
        args,
        ["/bin/sh", "scripts/linux-test.sh"],
        extra_env={"ELARIS_TEST_DISABLE_SANDBOX": "1"} if emulated else None,
    )


def linux_ui(args: argparse.Namespace) -> None:
    """Run Elaris in the container's virtual display. Open the printed noVNC address in a browser to see it."""
    port = _free_local_port(NOVNC_PORT)
    _run_in_container(
        args,
        [
            DISPLAY_ENTRYPOINT,
            "/bin/sh",
            "-c",
            './elaris setup --skip-system-deps && ./elaris run "$ELARIS_URL"',
        ],
        extra_env={"ELARIS_URL": args.url},
        publish=[f"127.0.0.1:{port}:{NOVNC_PORT}"],
        announce=f"Open http://127.0.0.1:{port}/vnc.html?autoconnect=1&resize=scale in a browser.",
        name=f"elaris-ui-{os.getpid()}",
    )


def _free_local_port(start: int) -> int:
    """The first port at or above `start` that nothing on this machine is listening on."""
    for port in range(start, start + 50):
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as probe:
            if probe.connect_ex(("127.0.0.1", port)) != 0:
                return port
    raise SystemExit(f"elaris: no free port found from {start} for the noVNC viewer")


def _run_in_container(
    args: argparse.Namespace,
    command: list[str],
    *,
    extra_env: dict[str, str] | None = None,
    publish: list[str] | None = None,
    announce: str | None = None,
    name: str | None = None,
) -> None:
    if platform.system() not in {"Darwin", "Linux"}:
        raise SystemExit("elaris: the Linux container commands run from macOS or Linux hosts")
    require("docker", "git")
    require_docker_daemon()

    architecture = args.arch or _host_architecture()
    docker_platform = PLATFORMS[architecture]
    jobs = engine._jobs(args.jobs)

    run("git", "submodule", "update", "--init", "--recursive")
    git_common_dir = _git_common_dir()
    ladybird_main_dir = _ladybird_main_dir(git_common_dir)

    uid = os.getuid()
    gid = os.getgid()
    workspace_key = hashlib.sha256(str(ROOT).encode()).hexdigest()[:10]
    # Each architecture keeps its own image and caches, so an emulated build never reuses native objects.
    suffix = f"{workspace_key}-{uid}-{gid}-{architecture}"
    volume_prefix = f"elaris-linux-{suffix}"
    image = f"elaris-linux:{suffix}"

    for directory in (ROOT / "build", ROOT / "target", ENGINE / "Build", ladybird_main_dir / "Build"):
        directory.mkdir(parents=True, exist_ok=True)

    ui.info(f"Building the {architecture} container image. Docker output follows.")
    run(
        "docker",
        "build",
        "--platform",
        docker_platform,
        "--progress=plain",
        "--file",
        str(DOCKERFILE),
        "--build-arg",
        f"USER_UID={uid}",
        "--build-arg",
        f"USER_GID={gid}",
        "--tag",
        image,
        ".",
        cwd=DEVCONTAINER_DIR,
    )

    mounts = ["--mount", f"type=bind,source={ROOT},target={ROOT}"]
    if not git_common_dir.is_relative_to(ROOT):
        mounts.extend(["--mount", f"type=bind,source={git_common_dir},target={git_common_dir}"])
    volumes = {
        "/home/vscode/.cargo": f"{volume_prefix}-cargo",
        "/home/vscode/.rustup": f"{volume_prefix}-rustup",
        # ccache's default directory. Keeping it in a volume lets rebuilds reuse compiled objects.
        "/home/vscode/.cache/ccache": f"{volume_prefix}-ccache",
        str(ROOT / "build"): f"{volume_prefix}-build",
        str(ROOT / "target"): f"{volume_prefix}-target",
        str(ENGINE / "Build"): f"{volume_prefix}-ladybird-build",
        str(ladybird_main_dir / "Build"): f"{volume_prefix}-vcpkg-build",
    }
    for target, source in volumes.items():
        mounts.extend(["--mount", f"type=volume,source={source},target={target},volume-nocopy"])

    ui.info("Preparing the container's build cache volumes.")
    run(
        "docker",
        "run",
        "--rm",
        "--init",
        "--platform",
        docker_platform,
        "--shm-size=2g",
        "--user",
        "0:0",
        *mounts,
        "--env",
        f"ELARIS_VOLUME_UID={uid}",
        "--env",
        f"ELARIS_VOLUME_GID={gid}",
        "--env",
        f"ELARIS_VCPKG_BUILD_DIR={ladybird_main_dir / 'Build'}",
        "--workdir",
        str(ROOT),
        image,
        "/bin/sh",
        str(PREPARE_VOLUMES),
    )

    container = ["docker", "run", "--rm", "--init", "--platform", docker_platform, "--shm-size=2g", *mounts]
    if name:
        container.extend(["--name", name])
    for port in publish or []:
        container.extend(["--publish", port])
    env = {"ELARIS_JOBS": jobs, **(extra_env or {})}
    for name, value in env.items():
        container.extend(["--env", f"{name}={value}"])
    container.extend(["--workdir", str(ROOT), image, *command])

    ui.info(f"Running on {architecture}. Container output follows.")
    if announce:
        ui.info(announce)
    try:
        run(*container)
    except (KeyboardInterrupt, SystemExit):
        # Stopping the client does not stop its container, which would keep the port and the display.
        if name:
            subprocess.run(["docker", "stop", "--time", "5", name], capture_output=True, check=False)
        raise


def _host_architecture() -> str:
    return "arm64" if platform.machine().lower() in {"arm64", "aarch64"} else "amd64"


def require_docker_daemon() -> None:
    result = subprocess.run(["docker", "info"], capture_output=True, text=True, check=False)
    if result.returncode:
        detail = (result.stderr or result.stdout).strip()
        message = "elaris: Docker is installed but its daemon is not available; start Docker and retry."
        raise SystemExit(f"{message}\n{detail}" if detail else message)



def _git_common_dir() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--git-common-dir"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode:
        raise SystemExit("elaris: could not locate the repository Git directory")
    path = Path(result.stdout.strip())
    if not path.is_absolute():
        path = ROOT / path
    return path.resolve()


def _ladybird_main_dir(git_common_dir: Path) -> Path:
    git_pointer = ENGINE / ".git"
    if git_pointer.is_dir():
        return ENGINE

    result = subprocess.run(
        ["git", "rev-parse", "--absolute-git-dir"],
        cwd=ENGINE,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode:
        return git_common_dir / "modules"

    main_dir = Path(result.stdout.strip()).parent.parent
    if main_dir.name == ".git":
        main_dir = main_dir.parent
    return main_dir.resolve()


def register_commands(commands) -> None:
    architecture_help = "Container architecture: amd64 (x86_64) or arm64 (default: this machine's)"
    for name, handler, help_text in (
        ("linux-build", linux_build, "Build Elaris for Linux in the container"),
        ("linux-test", linux_test, "Run the full Linux test suite in the container"),
        ("linux-ui", linux_ui, "Run Elaris in the container, viewable through noVNC"),
    ):
        command = commands.add_parser(name, help=help_text)
        command.add_argument("--arch", choices=sorted(PLATFORMS), help=architecture_help)
        command.add_argument("--jobs", help="Parallel build jobs (default: ELARIS_JOBS or 4)")
        command.set_defaults(handler=handler)
        if handler is linux_ui:
            command.add_argument("url", nargs="?", default="https://example.com", help="Address to open")
