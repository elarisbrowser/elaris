<p align="center"><img src="../elaris-brand/assets/logo.svg" alt="Photon" width="72" /></p>

# `elaris-shell`

The shared GPUI-CE browser window and platform adapters. It owns browser views, window lifecycle, native system integration, Engine session adaptation, and page-frame presentation for supported shell targets. Browser actions and presentation behavior are shared; OS-specific window and renderer implementations live under `src/platform/<target>/`.

## Run

The shell is started by `elaris-app`. After following the [contributor setup guide](../../CONTRIBUTOR_SETUP.md), launch it with:

```sh
elaris run
```

The shell composes browser rules from [`elaris-core`](../elaris-core/README.md) and profile files from [`elaris-storage`](../elaris-storage/README.md). See the [architecture guide](../../Documentation/Architecture.md) for its platform boundaries.
