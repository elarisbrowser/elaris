# Elaris Browser

Elaris is a small cross-platform browser shell built with Rust and [Rune](https://github.com/anomalyco/rune). The current app opens a dark native window with an editable address bar and an empty content area. Web rendering and Ladybird integration are not included yet.

## Build and run Elaris

Install the stable Rust toolchain, then run:

```sh
cargo run
```

Rune supports macOS and Linux. A working desktop session and GPU driver are required to open the native window on Linux.

## Ladybird

Ladybird is pinned as an unmodified upstream submodule at `vendor/ladybird`, revision `76bef6eee7bffd40c1895a2ee7332405fa1ef83d` (zero local patches). Initialize it after cloning Elaris:

```sh
git submodule update --init --recursive
```

Ladybird is built independently with its upstream `Meta/ladybird.py` script. The script expects a normal clone or Git worktree; it misidentifies the `.git` file used by a submodule as a linked worktree. Build from a detached worktree of the pinned revision so the upstream checkout stays clean:

```sh
git -C vendor/ladybird worktree add --detach /tmp/elaris-ladybird-build 76bef6eee7bffd40c1895a2ee7332405fa1ef83d
cd /tmp/elaris-ladybird-build
./Meta/ladybird.py build --jobs 8
```

The command builds Ladybird's Release configuration. To remove the extra checkout after building, run `git -C /path/to/elaris/vendor/ladybird worktree remove /tmp/elaris-ladybird-build` from any directory.

### macOS prerequisites

Install Xcode 26 or newer and the Xcode command-line tools. Install Ladybird's additional build tools with Homebrew:

```sh
brew install autoconf autoconf-archive automake ccache cmake libtool make nasm ninja pkg-config
```

Ladybird can use Xcode's compiler. Homebrew LLVM 22 is an alternative.

### Linux prerequisites

Ladybird requires a C++23-capable compiler, CMake 3.30 or newer, Qt 6.10 or newer, and the platform build packages below. The following package list is for Debian or Ubuntu; other distributions should use the matching package list in [Ladybird's build instructions](https://github.com/LadybirdBrowser/ladybird/blob/master/Documentation/BuildInstructionsLadybird.md).

```sh
sudo apt install autoconf autoconf-archive automake build-essential ccache cmake curl fonts-liberation2 git glslang-tools libdrm-dev libgl1-mesa-dev libncurses-dev libpulse-dev libtool libwayland-dev libxcb-icccm4-dev nasm ninja-build pkg-config python3-venv qt6-base-private-dev qt6-positioning-dev qt6-tools-dev-tools qt6-wayland tar unzip zip
```

Some distributions provide CMake or Qt versions below Ladybird's minimum; install newer versions before building when needed.

The submodule remains upstream-only. Any future Elaris C++ bridge will live outside `vendor/ladybird`; it is not implemented or linked to the Rune app yet.
