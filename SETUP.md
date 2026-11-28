# Development setup

These steps build Elaris from source. If you already have an alpha package, use the [install guide](docs/install.md) instead.

## 1. Install the bootstrap tools

You need Git and Python 3.9 or newer before `./elaris` can run.

- **macOS, interface only:** Install Apple's command-line tools with `xcode-select --install`. You don't need Homebrew or the full Xcode app.
- **macOS, full browser:** Install [Homebrew](https://brew.sh/) and full Xcode 26 or newer, and select that Xcode installation with `xcode-select`. If you still need Git or Python, run `brew install git python`.
- **Linux:** If Git or Python is missing, install it with apt, dnf, pacman, or zypper. Debian and Ubuntu: `sudo apt-get install git python3`. Package installation may ask for your sudo password.

If your default Python is too old, set `ELARIS_PYTHON` to the path of a newer interpreter when running the launcher.

## 2. Choose what to run

```sh
git clone https://github.com/elarisbrowser/elaris.git
cd elaris
```

| I want to… | Command |
| --- | --- |
| Build and launch the interface without the web engine | `./elaris setup --ui --run` |
| Build and launch the full browser | `./elaris setup --run` |
| Prepare the interface dependencies without building | `./elaris setup --ui` |
| Prepare the full dependencies without building | `./elaris setup` |

The interface route installs the Rust toolchain and any needed UI system packages, without checking out or compiling Ladybird. The full route also installs the engine prerequisites and initializes the pinned Ladybird submodule. On supported Linux distros, native package installation uses apt, dnf, pacman, or zypper. Local development doesn't need an Apple account, signing credentials, or a notarization run.

Use `--build` instead of `--run` to prepare and build without opening a window, for example `./elaris setup --ui --build` or `./elaris setup --build`. The first full build downloads dependencies and can take a while. Both routes need internet access for first-time setup. Subsequent launches:

```sh
./elaris run --ui
./elaris run https://example.com
```

### Already have the system packages?

Use `./elaris setup --skip-system-deps` with the same interface, build, or launch flags. This skips system package installation, not the remaining toolchain and project setup.

Full native setup verifies CMake 3.30+ and, on Linux, Qt 6.10+. Debian and Ubuntu full setup can add the signed LLVM apt repository and, on Ubuntu with older CMake, Kitware's signed apt repository. If your distro ships an older Qt, use Docker below or install a newer Qt development kit and point setup to it:

```sh
export CMAKE_PREFIX_PATH="$HOME/Qt/6.10.0/gcc_64"
export PKG_CONFIG_PATH="$CMAKE_PREFIX_PATH/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
./elaris setup --skip-system-deps
```

If you provision dependencies yourself, see [Ladybird's build requirements](https://github.com/LadybirdBrowser/ladybird/blob/master/Documentation/BuildInstructionsLadybird.md) and select versions compatible with the pinned revision. A native Linux desktop run needs a Vulkan driver, such as your system's GPU driver or Mesa.

### Linux window controls

On Linux, move the pointer to the top edge to reveal Elaris's custom toolbar. Set `ELARIS_LINUX_TITLEBAR=always` to keep it visible. Tiling window managers default to system decorations instead. Change this under Settings → Browser setup → Window frame (restart required), or override it with `ELARIS_LINUX_DECORATIONS=system` or `ELARIS_LINUX_DECORATIONS=elaris`.

## Linux in Docker

Install and start Docker first. You can run these on macOS or Linux:

| Command | What it does |
| --- | --- |
| `./elaris desktop` | Open the interface only in a lightweight Linux desktop. No web engine. |
| `./elaris linux-ui [URL]` | Build and open the full browser in a Linux desktop. |
| `./elaris linux-build` | Build the full Linux browser without opening a desktop. |
| `./elaris linux-test` | Run the Rust, bridge and bundled-browser checks. Results go to `linux-test-results/`. |

For `desktop` or `linux-ui`, open the local viewer URL printed in the terminal. Don't expose the viewer beyond your machine. Restart the command to see source edits. Docker build outputs are kept separate from native outputs.

The `linux-*` commands accept `--arch amd64` (x86_64) or `--arch arm64`; the default is the host architecture. Emulating amd64 on Apple Silicon is slow. These virtual desktop checks do not replace testing native desktop or GPU behavior.

For editor integration, open the repository as a [Dev Container](.devcontainer/devcontainer.json), then use `./elaris check` or `./elaris build` inside it.

## Useful commands

| Command | What it does |
| --- | --- |
| `./elaris check` | Run Rust formatting checks, type checking, Clippy, Rust tests, and CLI tests. |
| `./elaris format` | Format Rust and Elaris-owned C++ code. |
| `./elaris build` | Build Ladybird and Elaris without launching. |
| `./elaris build --rust` / `--ladybird` | Build just the interface or just Ladybird and the bridge, respectively. |
| `./elaris run --release` | Build and launch the optimized full browser. |
| `./elaris run --ui --linux-ui` | Preview the Linux layout without the engine. Not a test of Linux-specific behavior. |
| `./elaris clean` | Remove build outputs. Add `--rust` or `--ladybird` to narrow what is cleaned. |
| `./elaris engine status` | See the pinned Ladybird revision and build status. |

Pass `--jobs N` to `setup --build`, `setup --run`, `build`, `run`, or a `linux-*` command. Without an explicit value, Ladybird uses `ELARIS_JOBS` or four jobs, and Cargo chooses its own default. Use `./elaris --verbose build` to stream full output. Interactive build steps save their output to `build/logs/`.

For an isolated full-browser test, keep Elaris's data and Ladybird's profile in the same temporary directory. The explicit profile override protects the test even if your shell already points Ladybird elsewhere:

```sh
data_dir="$(mktemp -d)"
ELARIS_DATA_DIR="$data_dir" \
ELARIS_LADYBIRD_PROFILE_DIR="$data_dir/profile" \
./elaris run https://example.com
```

Plain `cargo run` also builds the interface only. `./elaris hot` supports compatible Rune function-body edits under `src/ui/` without restarting the process. It needs Dioxus CLI 0.7.10 (`dx`) and Python 3.11+; use `--patch 'src/app.rs'` to include another file.

See the [bridge guide](bridge/README.md) for C++ and engine checks, or the [release guide](docs/release.md) for packaging.
