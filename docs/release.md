# Release procedure

The [Release workflow](../.github/workflows/release.yml) builds macOS and Linux with Ladybird.

| Trigger | macOS signing | Result |
| --- | --- | --- |
| Manual `test` (default) | No credentials; verifies the signing plan | Unsigned test artifact; no release |
| Manual `release` on `main` | Developer ID, notarization and stapling | Signed test artifact; no release |
| Version tag from `main` | Developer ID, notarization and stapling | Draft pre-release; publish after review |
| Trusted canary on `main` | Developer ID, notarization and stapling | Rolling canary pre-release |

Signed-in readers of this public repo can download Actions artifacts until expiration, even if
there is no published release. Use a private delivery path for invited testers only.

## Cut a release

1. Set the version in `Cargo.toml`, then merge it to `main` with CI green.
2. Tag the commit with a version such as `v0.1.0-alpha.1` and push the tag.
3. Wait for the `Release` workflow. `verify` runs formatting, Clippy and tests, then the `macos` and `linux` jobs
   build, smoke-test and package each platform. The macOS signing job needs the protected `apple-signing` environment
   and fails if any Apple credential is missing or invalid.
4. Download each artifact's `.sha256` file and check it. Also validate the macOS DMG's stapled ticket, then run the
   browser from its archive on a clean machine; see [macOS code signing and notarization](macos-signing.md).
5. Publish the draft on GitHub, with release notes listing the limitations of the tested builds.

## Fast rebuilds

The macOS arm64 and Linux x86_64 Release jobs use two separate exact caches:

- **Patched Ladybird engine.** A hit skips native compilation and restores only runtime helpers, shared
  libraries, resources and macOS signing entitlements, never CMake or Ninja state. The key covers the Ladybird pin,
  bridge and patches, native build tooling, app version embedded in LibWebView, portable CPU flags, runner ABI and
  the Chromium HSTS preload data. Edits limited to Rust, signing, or documentation do not invalidate this cache.
- **Elaris executable.** An exact hit skips the release Cargo build. The key covers the Elaris commit and channel,
  Rust toolchain and build settings, and exact native engine key. The cached binary is checked against the bridge
  bytes, executable mode, architecture and checksum. The commit is embedded in the binary, so even a docs-only commit
  needs a new Elaris executable, but should still reuse Ladybird when its inputs match.

Release jobs still create fresh bundles, smoke-test the packaged browser and run the usual signing, notarization
and packaging checks. They never cache a signed DMG, credentials, or a final bundle. Release-source checks still run.

A missing, disabled or rejected exact cache falls back to building, using the usual compiler and vcpkg archives
when available. Use **Actions → Caches** to remove a bad exact key before warming it again; GitHub cannot overwrite
an existing key. After a Ladybird pin change, re-audit upstream mutable inputs and update the audited pin in
`tools/elaris_cli/release_engine.py` to reenable exact caching. If the HSTS fetch is unavailable, Release still builds.

Prime shared caches with a manual Release on trusted `main` without publishing. Independent tags cannot share one
another's caches. The first run for new keys is cold, and runner/SDK or preload changes can invalidate prior caches.
`BUILD-INFO.txt` records whether each component was built or reused and its producer run when available.

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
