# Ladybird bridge

The bridge is an opt-in C ABI over the pinned `vendor/ladybird` checkout. It does not add files to or patch that submodule. Ladybird must remain the top-level CMake project because its Rust rules use `CMAKE_SOURCE_DIR`; Elaris injects its target after Ladybird declares its libraries. Normal `cargo run` does not build or link Ladybird.

## Configure, build and test

On macOS, after Ladybird's vcpkg dependencies have been installed and built for the host architecture:

```sh
./elaris cpp-configure
cmake --build build/cpp-ladybird --target elaris_ladybird_smoke \
  WebContent RequestServer Hub ImageDecoder Compositor SystemProbe \
  AudioServer MediaServer VideoDecoderServer ProcessReaper -j 8
ctest --test-dir build/cpp-ladybird --output-on-failure -R elaris_ladybird_smoke
```

`cpp-configure` selects the local vcpkg triplet and Ladybird's pinned Rust toolchain. The smoke test needs Ladybird's resources and helper executables. CMake wires the helper runtime paths to the build libraries and the installed vcpkg libraries.

The smoke executable checks invalid arguments and a failed profile initialization, then creates three views in one application. Each view loads the deterministic `tests/smoke.html`, observes navigation, title, loading start/finish and ready-to-paint callbacks, pumps without blocking, resizes, waits with a timeout and until woken from another thread, and is closed and destroyed. It also destroys and reinitializes the application. The test verifies invalid dimensions and rejects a resize from a non-owner thread. CTest passes resource, profile and page paths to the executable.

## Rust boundary

The Rust wrapper and executable smoke example are behind the `ladybird-bridge` feature. After building the C++ bridge, link and run them on macOS with:

```sh
prefix="$PWD/vendor/ladybird/Build/release/vcpkg_installed/arm64-osx-dynamic"
ELARIS_LADYBIRD_LIB_DIR="$PWD/build/cpp-ladybird/lib" \
DYLD_LIBRARY_PATH="$PWD/build/cpp-ladybird/lib:$prefix/lib" \
  cargo run --locked --features ladybird-bridge --example ladybird_smoke -- \
  "$PWD/vendor/ladybird/Base/res" \
  "$PWD/build/cpp-ladybird/elaris-rust-profile" \
  "$PWD/bridge/tests/smoke.html"
```

The C ABI unsafe code is isolated in `src/ladybird.rs`. Its owner is neither `Send` nor `Sync`, views borrow that owner, and Rust callback panics are caught before returning through C. The example tests navigation/title/frame delivery, resize and an application reinitialization. Normal Cargo builds remain independent of the bridge.

The callback panic boundary regression test can be run with:

```sh
ELARIS_LADYBIRD_LIB_DIR="$PWD/build/cpp-ladybird/lib" \
DYLD_LIBRARY_PATH="$PWD/build/cpp-ladybird/lib:$prefix/lib" \
  cargo test --locked --features ladybird-bridge --lib \
  callback_panics_are_contained_at_the_ffi_boundary
```

## Linux GitHub Actions verification

`.github/workflows/ladybird-bridge-linux.yml` is manual-only. It checks out the pinned Ladybird submodule and uses Ladybird's `ladybird-ci:2026.11.12` Linux build image, which supplies its documented compiler, Qt and system build dependencies. If clang-tidy 22 is absent, the workflow downloads and extracts its packages under the runner's temporary directory without requiring root. It verifies Xvfb, Mesa software rendering and ccache, builds Ladybird's vcpkg dependencies and the bridge/helper executables, then runs the C++ and Rust smoke tests under Xvfb. It also runs clang-tidy on the Elaris bridge, feature-enabled Rust Clippy and FFI boundary tests, and checks that helper processes have exited after each runtime test. Everyday Rust formatting/check/Clippy/tests and C++ formatting remain in fast CI. Each costly build/test stage has its own timeout; CMake and vcpkg logs are uploaded when a run fails.

To run full Linux verification, open **Actions → Ladybird bridge (Linux) → Run workflow**, select the branch to test, and start the workflow. Dependency setup, C++ compilation, formatting/static analysis, C++ runtime, Rust checks and Rust runtime are separate named steps so the first failing step identifies the failure class. A failed run uploads available vcpkg, CMake, CTest and Xvfb logs.

Everyday push and pull request checks remain in `.github/workflows/ci.yml`; they run Rust formatting, check, Clippy and tests plus Elaris C++ formatting without configuring or building Ladybird. The manual workflow caches Ladybird's vcpkg checkout, vcpkg binary packages and ccache. It deliberately does not cache the installed vcpkg tree or CMake build directory: vcpkg restores installed dependencies from its ABI-keyed binary package cache, while ccache safely reuses compiler outputs and is capped at 3 GiB. Cache keys include the Ladybird CI image and dependency manifests, and unique run IDs let successful runs update caches for subsequent runs. Bridge-specific Rust Clippy and callback boundary tests run in the manual workflow because they require the compiled FFI library; the everyday checks do not. Ladybird's helper resolves shared build paths through the parent repository's `.git/modules` for this nested submodule, so the workflow sets and caches those paths explicitly.

Automatic runs can be added later by defining `push` and `pull_request` path filters beside `workflow_dispatch`. Include changes to `.github/workflows/ladybird-bridge-linux.yml`, `bridge/**`, the `vendor/ladybird` submodule pointer, `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/lib.rs`, the Rust bridge module, the smoke example, and `scripts/elaris_cli/**`.

## Verified behavior and limitations

- macOS Apple Silicon: configured and compiled against Ladybird `76bef6eee7bffd40c1895a2ee7332405fa1ef83d`; the C++ CTest smoke passed, including helper-process cleanup, and the Rust example linked and ran successfully. The Rust callback-panic boundary regression test also passed.
- Linux: the GitHub Actions workflow is configured, but has not yet had a remote run. Linux configure, compile, helper startup, integration callbacks, shutdown, and Rust dynamic linking remain unverified until the workflow passes.
- `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked` passed. Feature-enabled Rust check, Clippy and the callback-panic test passed. `clang-format` and the bridge's compiler warning-as-error build passed. `clang-tidy` is not installed in this environment, so static analysis with it remains unrun.
- Runtime initialization reports that optional site-compatibility data is missing from this checkout's resource package. The local HTML integration test still passes.
- AppleClang needs one narrow warning exception for the statement-expression expansion inside Ladybird's `WEB_VIEW_APPLICATION` macro. No other Elaris C++ warnings were suppressed.

The bridge copies frames out as RGBA and leaves showing them to the host. A host can give Ladybird a thread of its own: `elaris_ladybird_wait` blocks that thread until Ladybird has work, and `elaris_ladybird_wake` interrupts it from any other thread. Elaris does this so the UI thread never waits on Ladybird; see `docs/performance/webview.md`.
