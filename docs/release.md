# Release procedure

The `Release` workflow (`.github/workflows/release.yml`) builds the macOS and Linux bundles, each with Ladybird's
engine, and attaches them to a **draft pre-release**. The macOS artifact is Developer ID signed, notarized, stapled,
and verified before upload. Nothing is public until someone checks the artifacts and publishes the draft by hand.

## Cut a release

1. Set the version in `Cargo.toml`, then merge it to `main` with CI green.
2. Tag the commit with a version such as `v0.1.0-alpha.1` and push the tag.
3. Wait for the `Release` workflow. `verify` runs formatting, Clippy and tests, then the `macos` and `linux` jobs
   build, smoke-test and package each platform. The macOS signing job needs the protected `apple-signing` environment
   and fails if any Apple credential is missing or invalid.
4. Download each artifact's `.sha256` file and check it. Also validate the macOS DMG's stapled ticket, then run the
   browser from its archive on a clean machine; see [macOS code signing and notarization](macos-signing.md).
5. Publish the draft on GitHub, with the release notes listing the known limitations below.

`workflow_dispatch` defaults to `test`: it runs the macOS smoke tests and signing-plan inspection, then uploads an
explicitly named unsigned test DMG without Apple credentials. Selecting `release` runs the signed macOS path from
trusted `main` but still does not create a GitHub release.

## Fast rebuilds

The macOS arm64 and Linux x86_64 Release jobs look for an **exact** cache of the compiled, patched Ladybird
engine. A hit skips the native build and keeps only its helpers, shared libraries, resources and macOS signing
entitlements; it never restores CMake, Ninja, Cargo, a previous .app, or a previous Linux install prefix.
Elaris is built from the current Rust sources and bundled fresh on every release. Existing signing, relocation
checks and smoke tests still run.

- The engine key covers the Ladybird pin, bridge and native build inputs, portable CPU flags, runner toolchain
  and ABI, and the Chromium HSTS preload data CMake embeds. A normal Rust-only edit does not invalidate it,
  but changing the Cargo package version does because LibWebView embeds that version. Signing scripts and credential
  changes do not invalidate the Ladybird build cache.
- If the engine key is missing, the existing build runs with the usual compiler and vcpkg caches. If a restored
  engine is incomplete or corrupt, the workflow logs why and builds native code instead. Use **Actions →
  Caches** to remove a bad exact key before warming it again; GitHub cannot overwrite an existing key.
- When changing the Ladybird pin, re-audit upstream mutable build inputs and update the audited pin in
  `tools/elaris_cli/release_engine.py`. Until then, or when the HSTS fetch is unavailable, Release runs normally
  without exact engine reuse.
- To prime the cache for future tags, manually run Release on trusted `main` without publishing. Separate
  tags cannot share each other's GitHub Actions caches. The first run is cold; runner/SDK and preload updates
  may also cause later misses.

`BUILD-INFO.txt` reports whether the engine was compiled or reused, its input key and preload hash, and the
producer commit and Actions run where available. A changed checksum or fresh bundle name alone does not prove
the engine was rebuilt.

## Before tagging: third-party notices

`THIRD_PARTY_NOTICES.md` covers the Rust crates and the bundled C++ components of both platforms. The Linux bundle is
built in the container, so its vcpkg metadata is staged out of the container's volumes first:

1. Build the x86_64 bundle with `./elaris linux-build --arch amd64`, or `./elaris linux-test --arch amd64`.
2. Stage its library list and vcpkg metadata into `build/notices-inputs/x64-linux-dynamic` (read-only from the build
   volumes; see the commit that added the Linux notices for the exact `docker run` command).
3. Regenerate the notices on the host with `./elaris bundle` run on macOS and `python3 tools/notices.py`, then check
   that `python3 tools/notices.py --check` passes.

## Versions

Releases use SemVer pre-release tags such as `v0.1.0-alpha.1`. `Cargo.toml` carries the same version, and the release
artifacts and `BUILD-INFO.txt` use the tag. The macOS app bundle keeps the plain `0.1.0` in `Info.plist`, because Apple's
bundle version fields accept only numbers and dots.

## What each artifact contains

| Platform | File | Contents |
| --- | --- | --- |
| macOS, Apple Silicon | `Elaris-<version>-macos-arm64.dmg` | `Elaris.app`: browser, Ladybird helpers, resources, and linked libraries, Developer ID signed and notarized; the DMG ticket is stapled |
| Linux, x86_64 | `Elaris-<version>-linux-x86_64.tar.gz` | A prefix tree: `bin/elaris`, `libexec/` helpers, `lib/` libraries, `share/` resources, desktop entry and icons |
| Linux, x86_64 | `Elaris-<version>-linux-x86_64.AppImage` | Portable AppImage built from the same tested prefix tree, with Ladybird resources and bundled libraries |

Each package comes with a `.sha256` checksum and a `BUILD-INFO.txt` that records the Elaris commit, the pinned Ladybird
commit, the Rune revision, and the toolchain used.

## Limitations of this release process

- macOS releases target Apple Silicon only. Signed distribution requires the Apple Developer membership and protected
  `apple-signing` secrets described in [the signing guide](macos-signing.md); release signing intentionally fails closed
  until those credentials are configured.
- Intel Macs are not supported for the alpha, and Linux ARM64 is not built. The signing script discovers Mach-O
  architectures at runtime and does not assume arm64.
- The minimum macOS version is 14.4, the lowest version Ladybird supports.
- Linux needs a Vulkan driver at run time, which the system's GPU driver or Mesa provides.
- The Linux x86_64 release is built on Ubuntu 24.04 and checks that bundled ELF files require no newer than glibc 2.39.
  This matches the Ubuntu 24.04 baseline in Ladybird's pinned release matrix. Older distributions such as Debian 12 and
  RHEL 9 remain outside the supported baseline. The release workflow smoke-tests the relocated bundle under Xvfb; real
  Wayland and X11 desktop verification is still needed.
- Linux support targets both Wayland and X11 sessions. Neither has been tested on a real desktop yet.
- The pinned Ladybird and Rune revisions are recorded in `BUILD-INFO.txt`. Both must be pinned before tagging.
