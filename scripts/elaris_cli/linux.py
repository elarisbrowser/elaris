"""Run the integrated Elaris build in the Linux devcontainer image."""

import argparse
import hashlib
import os
import platform
import subprocess
from pathlib import Path

from . import engine
from .common import ENGINE, ROOT, require, run

DEVCONTAINER_DIR = ROOT / ".devcontainer"
DOCKERFILE = DEVCONTAINER_DIR / "Dockerfile"


def linux_build(args: argparse.Namespace) -> None:
    if platform.system() not in {"Darwin", "Linux"}:
        raise SystemExit("elaris: linux-build is supported from macOS or Linux hosts")
    jobs = engine._jobs(args.jobs)
    require("docker", "git")
    _require_docker_daemon()

    run("git", "submodule", "update", "--init", "--recursive")
    git_common_dir = _git_common_dir()
    ladybird_main_dir = _ladybird_main_dir(git_common_dir)

    uid = os.getuid()
    gid = os.getgid()
    workspace_key = hashlib.sha256(str(ROOT).encode()).hexdigest()[:10]
    volume_prefix = f"elaris-linux-{workspace_key}-{uid}-{gid}"
    image = f"elaris-linux:{workspace_key}-{uid}-{gid}"

    build_dirs = (ROOT / "build", ROOT / "target", ENGINE / "Build", ladybird_main_dir / "Build")
    for directory in build_dirs:
        directory.mkdir(parents=True, exist_ok=True)

    print("Building the Linux image. Docker output follows.", flush=True)
    run(
        "docker",
        "build",
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
        mounts.extend(
            ["--mount", f"type=bind,source={git_common_dir},target={git_common_dir}"]
        )

    volumes = {
        "/home/vscode/.cargo": f"{volume_prefix}-cargo",
        "/home/vscode/.rustup": f"{volume_prefix}-rustup",
        str(ROOT / "build"): f"{volume_prefix}-build",
        str(ROOT / "target"): f"{volume_prefix}-target",
        str(ENGINE / "Build"): f"{volume_prefix}-ladybird-build",
        str(ladybird_main_dir / "Build"): f"{volume_prefix}-vcpkg-build",
    }
    for target, source in volumes.items():
        mounts.extend(
            ["--mount", f"type=volume,source={source},target={target},volume-nocopy"]
        )

    print("Preparing Linux build cache volumes.", flush=True)
    run(
        "docker",
        "run",
        "--rm",
        "--init",
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
        str(ROOT / ".devcontainer" / "prepare-volumes.sh"),
    )

    container_command = ["docker", "run", "--rm", "--init", "--shm-size=2g", *mounts]
    container_command.extend(
        [
            "--workdir",
            str(ROOT),
            "--env",
            f"ELARIS_JOBS={jobs}",
            image,
            "/bin/sh",
            "-c",
            './elaris setup --skip-system-deps && ./elaris build --jobs "$ELARIS_JOBS"',
        ]
    )
    print("Running the Linux setup and build. Build output follows.", flush=True)
    run(*container_command)


def _require_docker_daemon() -> None:
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
    command = commands.add_parser(
        "linux-build",
        help="Build Elaris for Linux in Docker and stream the build output",
    )
    command.add_argument("--jobs", help="Parallel build jobs (default: ELARIS_JOBS or 4)")
    command.set_defaults(handler=linux_build)
