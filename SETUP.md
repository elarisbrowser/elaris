# Development setup

## Before running `./elaris`

The launcher needs Python 3.9 or newer before it can start. Git is needed to clone the repository and check out Ladybird. If `python3` selects an older interpreter, point the launcher to a newer one:

```sh
ELARIS_PYTHON=/path/to/python3.12 ./elaris setup
```

Install those bootstrap tools using your platform's package manager if they are missing:

```sh
brew install python git                         # macOS
sudo apt-get install python3 python3-venv git   # Debian or Ubuntu
sudo dnf install python3 git                    # Fedora
sudo pacman -S python git                       # Arch or Manjaro
sudo zypper install python3 git                 # openSUSE
```

macOS also needs [Homebrew](https://brew.sh/) and Xcode 26 or newer. Linux setup supports apt, dnf, pacman, and zypper.

## One-time setup

Run this from the repository root:

```sh
./elaris setup
```

Setup installs the Ladybird system build packages through Homebrew or the detected Linux package manager, installs rustup and the stable plus pinned Ladybird Rust toolchains, and checks out the exact Ladybird revision recorded by this repository. It also makes the submodule's local Git pointer absolute so Ladybird can find its shared vcpkg cache when building from a submodule checkout. This only changes ignored Git metadata, not Ladybird source or the pinned revision. On Linux, `sudo` prompts for your password when it needs permission to install packages. The password is handled by `sudo`; Elaris does not read or save it.

The automatic package install supports macOS with Homebrew, Debian or Ubuntu with apt, Fedora with dnf, Arch or Manjaro with pacman, and openSUSE with zypper. Other distributions can install prerequisites from [Ladybird's build instructions](https://github.com/LadybirdBrowser/ladybird/blob/master/Documentation/BuildInstructionsLadybird.md), then run:

```sh
./elaris setup --skip-system-deps
```

Setup verifies CMake 3.30 or newer on all platforms, Qt 6.10 or newer on Linux, and Xcode 26 or newer on macOS. Debian and Ubuntu setup adds LLVM's signed apt repository for Ladybird's supported C++ compiler and adds Kitware's signed apt repository on Ubuntu only when the system CMake is too old. Some Linux distributions still ship Qt older than 6.10; install a newer Qt development kit and set its paths before rerunning setup. For example:

```sh
export CMAKE_PREFIX_PATH="$HOME/Qt/6.10.0/gcc_64"
export PKG_CONFIG_PATH="$CMAKE_PREFIX_PATH/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
./elaris setup --skip-system-deps
```

The setup command never adds a Ladybird fork or changes its pinned revision.

## Optional Linux devcontainer

Open the repository in a Dev Container to use the same Ladybird CI image as the Linux bridge workflow. First creation downloads the image and runs `./elaris setup --skip-system-deps`. Cargo, Rust toolchains, and Linux build artifacts persist in Docker volumes, separate from host build outputs. Run `./elaris check` or `./elaris build` inside the container. It is intended for Linux builds and checks, not native desktop or GPU testing.

From the host terminal, `./elaris linux-build` runs the same setup and integrated build in Docker and streams its output. Pass `--jobs N` to change parallelism. Docker must be installed and running.

## Build and run

After setup:

```sh
./elaris build             # build Ladybird dependencies, bridge, and Elaris
./elaris run               # build what is needed, then launch Elaris
./elaris run https://example.com
```

The first build bootstraps Ladybird's pinned vcpkg checkout and compiles its third-party dependencies and runtime helper processes. It requires an internet connection and can take a while; allow several gigabytes for source and build artifacts. The default build parallelism is four jobs. Set `ELARIS_JOBS=8` or pass `--jobs 8` to `build` or `run` to change it.

For an isolated development or test run, point Elaris and Ladybird at an absolute temporary data
directory. This remains isolated even if your shell already sets a Ladybird profile override:

```sh
isolated_data_dir="$(mktemp -d)"
ELARIS_DATA_DIR="$isolated_data_dir" \
ELARIS_LADYBIRD_PROFILE_DIR="$isolated_data_dir/profile" \
./elaris run https://example.com
```

For ordinary runs, `ELARIS_DATA_DIR` also supplies the default Ladybird profile root. The explicit
`ELARIS_LADYBIRD_PROFILE_DIR` override remains available when an engine profile should live
elsewhere.

To work on Rune UI without compiling Ladybird:

```sh
./elaris build --ui
./elaris run --ui
./elaris hot
```

`./elaris hot` uses Rune's guarded live-edit launcher and keeps the UI process and app state alive for compatible function-body edits under `src/ui/`. Add `--patch 'src/app.rs'` or another glob to include more source files. It requires Dioxus CLI 0.7.10 (`dx`) and Python 3.11+; if the default `python3` is older, the command selects a versioned interpreter from `PATH`.

Direct `cargo run` also remains a UI-only build and does not require the Ladybird submodule or bridge. The hot-reload feature is enabled only by `./elaris hot`.

## Contributor commands

| Command | What it does |
| --- | --- |
| `./elaris setup` | Install prerequisites, Rust toolchains, and the pinned Ladybird submodule |
| `./elaris build` | Build the integrated browser, including Ladybird and its bridge |
| `./elaris run [URL]` | Build the integrated browser and open it |
| `./elaris check` | Run `cargo fmt --check`, `cargo check`, Clippy, and Rust tests |
| `./elaris format` | Format Rust and Elaris-owned C++ sources |
| `./elaris engine status` | Inspect the pinned Ladybird revision and local build status |
| `./elaris cpp-tidy` | Run clang-tidy on Elaris-owned bridge code after configuring it |

For focused FFI work and bridge smoke tests, see [bridge/README.md](bridge/README.md). For code organization and thread ownership, see [docs/architecture.md](docs/architecture.md).
