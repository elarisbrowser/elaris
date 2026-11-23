# Release procedure

The `Release` workflow (`.github/workflows/release.yml`) builds the macOS and Linux bundles, each with Ladybird's
engine, and attaches them to a **draft pre-release**. Nothing is public until someone checks the artifacts and
publishes the draft by hand.

## Cut a release

1. Set the version in `Cargo.toml`, then merge it to `main` with CI green.
2. Tag the commit with a version such as `v0.1.0-alpha.1` and push the tag.
3. Wait for the `Release` workflow. `verify` runs formatting, Clippy and tests, then the `macos` and `linux` jobs
   build, smoke-test and package each platform.
4. Download each artifact's `.sha256` file and check it, then run the browser from its archive on a clean machine.
5. Publish the draft on GitHub, with the release notes listing the known limitations below.

`workflow_dispatch` runs the same jobs for a chosen branch and uploads the artifacts without creating a release.

## Before tagging: third-party notices

`THIRD_PARTY_NOTICES.md` covers the Rust crates and the bundled C++ components of both platforms. The Linux bundle is
built in the container, so its vcpkg metadata is staged out of the container's volumes first:

1. Build the x86_64 bundle with `./elaris linux-build --arch amd64`, or `./elaris linux-test --arch amd64`.
2. Stage its library list and vcpkg metadata into `build/notices-inputs/x64-linux-dynamic` (read-only from the build
   volumes; see the commit that added the Linux notices for the exact `docker run` command).
3. Regenerate the notices on the host with `./elaris bundle` run on macOS and `python3 scripts/notices.py`, then check
   that `python3 scripts/notices.py --check` passes.

## Versions

Releases use SemVer pre-release tags such as `v0.1.0-alpha.1`. `Cargo.toml` carries the same version, and the release
artifacts and `BUILD-INFO.txt` use the tag. The macOS app bundle keeps the plain `0.1.0` in `Info.plist`, because Apple's
bundle version fields accept only numbers and dots.

## What each artifact contains

| Platform | File | Contents |
| --- | --- | --- |
| macOS, Apple Silicon | `Elaris-<version>-macos-arm64.dmg` | `Elaris.app`: the browser, Ladybird's helper processes, its resources, and the libraries it links, ad-hoc signed |
| Linux, x86_64 | `Elaris-<version>-linux-x86_64.tar.gz` | A prefix tree: `bin/elaris`, `libexec/` helpers, `lib/` libraries, `share/` resources, desktop entry and icons |

Each archive comes with a `.sha256` checksum and a `BUILD-INFO.txt` that records the Elaris commit, the pinned Ladybird
commit, the Rune revision, and the toolchain used.

## Limitations of this release process

- Apple builds are ad-hoc signed. They are not notarized, so Gatekeeper will ask for approval on first launch.
- macOS builds target Apple Silicon only. Intel Macs are not supported for the alpha, and Linux ARM64 is not built.
- The minimum macOS version is 14.4, the lowest version Ladybird supports.
- Linux needs a Vulkan driver at run time, which the system's GPU driver or Mesa provides.
- The Linux x86_64 build runs in Ladybird's CI image. Local verification was done on arm64 under Docker, so the first
  run of the workflow is the first x86_64 check.
- The pinned Ladybird and Rune revisions are recorded in `BUILD-INFO.txt`. Both must be pinned before tagging.
