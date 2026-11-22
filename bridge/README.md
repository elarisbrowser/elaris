# Ladybird bridge

For the normal local workflow, run `./elaris setup` once and then `./elaris build`; the wrapper installs prerequisites, prepares the pinned checkout, builds Ladybird's dependencies, and compiles the bridge and helper processes described below. The bridge is an opt-in C ABI over `vendor/ladybird`. The vendored checkout stays pristine. For Peek, CMake applies [a small patch](patches/peek.patch) to copies of two source files in the build directory using `patch`. The patch adds Shift-only pointer activation for HTTP, HTTPS and file links through Ladybird’s existing verified link callback. Normal links, downloads and other modifier combinations keep their upstream activation paths. Ladybird must remain the top-level CMake project because its Rust rules use `CMAKE_SOURCE_DIR`; Elaris injects its target after Ladybird declares its libraries. Direct `cargo run` remains a UI-only build and does not link Ladybird.

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

The `elaris_ladybird_engine_features` test checks two engine behaviours Elaris relies on without its own code. The bundled site compatibility rules must load and hide Ladybird from `cnn.com` while leaving `example.com` alone, and `tests/sample.pdf` must open in the pdf.js viewer without a download. Run it with `ctest --test-dir build/cpp-ladybird --output-on-failure -R elaris_ladybird_engine_features`.

The focused `elaris_ladybird_form_pickers` test loads a webpage and answers its picker requests through the bridge. It checks file contents, multiple files, color and date choices, grouped select options, disabled options, and cancellation. Run it with `ctest --test-dir build/cpp-ladybird --output-on-failure -R elaris_ladybird_form_pickers`. The pinned Ladybird revision accepts individual files and has no directory-upload request.

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

The C ABI unsafe code is isolated in `src/ladybird/bridge.rs`. Its owner is neither `Send` nor `Sync`, views borrow that owner, and Rust callback panics are caught before returning through C. The example tests navigation/title/frame delivery, resize and an application reinitialization. Normal Cargo builds remain independent of the bridge.

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

Everyday push and pull request checks remain in `.github/workflows/ci.yml`; they run Rust formatting, check, Clippy and tests plus Elaris C++ formatting without configuring or building Ladybird. The manual workflow caches Ladybird's vcpkg checkout and binary packages, ccache (capped at 3 GiB), Cargo downloads, and the Ladybird, bridge and Rust build directories. It restores the latest cache and explicitly saves progress even when a later build or smoke test fails, so retries can reuse completed dependency work and continue incremental builds. Cache keys are scoped to the Ladybird CI image and dependency manifests, with unique run IDs so each attempt can publish an updated snapshot. Bridge-specific Rust Clippy and callback boundary tests run in the manual workflow because they require the compiled FFI library; the everyday checks do not. Ladybird's helper resolves shared build paths through the parent repository's `.git/modules` for this nested submodule, so the workflow sets and caches those paths explicitly.

Automatic runs can be added later by defining `push` and `pull_request` path filters beside `workflow_dispatch`. Include changes to `.github/workflows/ladybird-bridge-linux.yml`, `bridge/**`, the `vendor/ladybird` submodule pointer, `Cargo.toml`, `Cargo.lock`, `build.rs`, `src/lib.rs`, the Rust bridge module, the smoke example, and `scripts/elaris_cli/**`.

## Verified behavior and limitations

- macOS Apple Silicon: configured and compiled against Ladybird `76bef6eee7bffd40c1895a2ee7332405fa1ef83d`; the C++ CTest smoke passed, including helper-process cleanup, and the Rust example linked and ran successfully. The Rust callback-panic boundary regression test also passed.
- Linux (arm64, `./elaris linux-build` in the devcontainer on Apple Silicon): configure, compile, and all four `elaris_ladybird_*` CTest executables pass, as do feature-enabled Clippy, the Rust FFI tests and the Rust smoke example. The GitHub Actions workflow has not yet had a passing remote run, and x86_64 has not been built locally.
- Packaged runtime: the bridge picks Ladybird's helper directory at run time (beside the executable on macOS, or `libexec` under the prefix on Linux) and falls back to the build tree. Resources come from Ladybird's own layout unless `ELARIS_LADYBIRD_RESOURCE_DIR` or `resource_dir` overrides them.
- `cargo fmt --check`, `cargo check --locked`, `cargo clippy --locked -- -D warnings`, and `cargo test --locked` passed. Feature-enabled Rust check, Clippy and the callback-panic test passed. `clang-format` and the bridge's compiler warning-as-error build passed. `clang-tidy` is not installed in this environment, so static analysis with it remains unrun.
- Ladybird loads `WebCompat.json` and its pdf.js viewer while the application is created, from the `Ladybird.app` resources next to `WebContent`. `platform_init()` replaces the resource root the host passes in during `create()`, so the bridge's resource directory does not supply either of them. Both work from a build tree; a packaged build must keep that bundle layout, or change Ladybird's initialization.
- AppleClang needs one narrow warning exception for the statement-expression expansion inside Ladybird's `WEB_VIEW_APPLICATION` macro. No other Elaris C++ warnings were suppressed.

Elaris also applies [disable-reporting.patch](patches/disable-reporting.patch) to LibWebView. It keeps Ladybird's crash capture running but never keeps or submits crash reports, and it makes web-compatibility report submissions fail before any network request. The two switches are `reporting_enabled` in `ReportSubmission.cpp` and the `crash_report_sending()` getter in `Settings.h`.

If reporting is enabled later, only engine crashes can be reported. `ElarisApplication` overrides `captures_browser_crashes()` to `false`, so the Elaris host process never installs Ladybird's browser crash handler. Engine processes (WebContent, RequestServer, ImageDecoder, Compositor, and the other helpers) still capture crashes. Hub, which is part of the engine, still captures its own. Every report names `application` (`elaris`), `application_version` (from `Cargo.toml`) and `engine` (`ladybird`), next to the Ladybird build information. A future reporting backend can change the two switches and set `ReportSubmission::set_post_function` to its own transport.

The bridge copies frames out as RGBA and leaves showing them to the host. A host can give Ladybird a thread of its own: `elaris_ladybird_wait` blocks that thread until Ladybird has work, and `elaris_ladybird_wake` interrupts it from any other thread. Elaris does this so the UI thread never waits on Ladybird; see `docs/performance/webview.md`.

## Website permission verification

Serve the fixture on loopback so Ladybird considers it a secure context.

```sh
python3 -m http.server 41729 --bind 127.0.0.1 --directory bridge/tests
```

In a second terminal, set the bridge library paths as above and run:

```sh
ELARIS_TEST_LOCATION=1 cargo run --locked --features ladybird-bridge \
  --example ladybird_permissions -- "$PWD/vendor/ladybird/Base/res" \
  "$(mktemp -d)" http://127.0.0.1:41729/permissions.html
```

The fixture selects a deterministic position source for the process. To check the
browser UI with an isolated profile, launch with `ELARIS_TEST_LOCATION=1`,
`ELARIS_DATA_DIR` set to a new absolute directory, and
`./elaris run http://127.0.0.1:41729/permissions.html`. Request location, allow
it, and repeat the request. Reset Location in site controls, request it again,
block it, and repeat the request. Reset again, request location, then follow
**Navigate away** while the prompt is open. The prompt should disappear and a
fresh request should ask again.

The microphone and notifications buttons use the real Web APIs. For a
microphone OS-denial check on macOS, deny Elaris access in System Settings,
reset the site's microphone override, and request it. Elaris should show a
system denial notice and keep the site override unset. Then set the site
override to Block and request again to check browser denial. Enable microphone
access in the system and allow the site to check the activity indicator and
**Stop microphone**. Notifications use Notification Center in the macOS app
bundle and the desktop notification daemon on the Linux session bus.

Defaults can be edited in Elaris's `config.toml` using `ask` or `block`.
Elaris applies a blocked default in the bridge by refusing the prompt, so a
saved site decision still takes precedence, Ladybird's own settings and system
authorization still apply, and `navigator.permissions.query` reports `prompt`
for blocked-by-default sites.

```toml
[pages]
autoplay = "block-audio"   # allow, block-audio or block

[permission_defaults]
location = "ask"
microphone = "ask"
notifications = "block"

[permissions."https://example.com"]
notifications = "allow"
autoplay = "allow"
```

The same fixture server also serves `external.html` (external link prompts and the missing-handler
notice) and `storage.html` (site data clearing); the example exercises both.

## Content blocking

Ladybird's content blocker is off until `config.toml` or the settings page turns it on. The four built-in lists are independent; none is used until enabled.

```toml
[content_blocking]
enabled = false      # download the enabled lists and block matching requests
easylist = true      # advertising
easyprivacy = true   # trackers
cookie_notices = false
annoyances = false
```

Turning blocking on starts the download of every enabled list that has never been fetched or is older than a day. Enabling a list while blocking is on fetches it too: `elaris_ladybird_set_content_blocking` turns the engine's filter list updates off and on again, because Ladybird otherwise fetches a new list only at its daily check. A page loaded before its list arrives is not blocked, so reload it afterwards. Custom list URLs and per-site exceptions exist in Ladybird's settings but are not exposed yet.
