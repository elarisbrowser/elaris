# Elaris development setup

## Elaris

Install the stable Rust toolchain and run the Rune app from the repository root:

```sh
cargo run
```

This app runs independently of Ladybird. It does not require, link, or launch the web engine.

## Ladybird

Ladybird is an unmodified upstream Git submodule at `vendor/ladybird`, pinned to revision `76bef6eee7bffd40c1895a2ee7332405fa1ef83d`. There are no local Ladybird patches.

Initialize the pinned submodule after cloning Elaris:

```sh
git submodule update --init --recursive
```

Ladybird's build script misidentifies the `.git` file used by a submodule as a linked worktree. Use a detached worktree for the build; this keeps `vendor/ladybird` clean and lets the upstream script resolve its build paths:

```sh
git -C vendor/ladybird worktree add --detach /tmp/elaris-ladybird-build 76bef6eee7bffd40c1895a2ee7332405fa1ef83d
cd /tmp/elaris-ladybird-build
./Meta/ladybird.py build --jobs 8
```

This builds Ladybird's Release configuration. Adjust `--jobs` for the available memory and CPU count. Use another temporary path if `/tmp/elaris-ladybird-build` already exists.

To remove the worktree after the build, run this from the Elaris repository root:

```sh
git -C vendor/ladybird worktree remove /tmp/elaris-ladybird-build
```

### macOS prerequisites

Install Xcode 26 or newer, including its command-line tools. Install Ladybird's additional build tools with Homebrew:

```sh
brew install autoconf autoconf-archive automake ccache cmake libtool make nasm ninja pkg-config
```

Ladybird can use Xcode's compiler. Homebrew LLVM 22 is an alternative.

### Linux prerequisites

Ladybird requires a C++23-capable compiler, CMake 3.30 or newer, and Qt 6.10 or newer. This package list is for Debian or Ubuntu:

```sh
sudo apt install autoconf autoconf-archive automake build-essential ccache cmake curl fonts-liberation2 git glslang-tools libdrm-dev libgl1-mesa-dev libncurses-dev libpulse-dev libtool libwayland-dev libxcb-icccm4-dev nasm ninja-build pkg-config python3-venv qt6-base-private-dev qt6-positioning-dev qt6-tools-dev-tools qt6-wayland tar unzip zip
```

Some distributions provide CMake or Qt versions below Ladybird's minimum; install newer versions before building when needed. For other distributions, use the matching package list in [Ladybird's official build instructions](https://github.com/LadybirdBrowser/ladybird/blob/master/Documentation/BuildInstructionsLadybird.md).

The future Elaris C++ bridge will live outside `vendor/ladybird`. No bridge or engine integration is implemented yet.
