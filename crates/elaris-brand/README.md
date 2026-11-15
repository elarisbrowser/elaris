# `elaris-brand`

<p align="center"><img src="assets/logo.svg" alt="Elaris" width="72" /></p>

The single source for the Elaris browser name, engine name, internal page scheme, profile folder, app icon and planet logo asset. Other crates use these values instead of repeating brand strings.

## Use

```rust
use elaris_brand::{DATA_FOLDER, ENGINE_NAME, LEGACY_DATA_FOLDER, LEGACY_PAGE_SCHEME, NAME, PAGE_SCHEME};

assert_eq!(NAME, "elaris.");
assert_eq!(ENGINE_NAME, "elaris. Engine");
let settings_url = format!("{PAGE_SCHEME}://settings");
assert_eq!(LEGACY_PAGE_SCHEME, "photon");
assert_eq!(DATA_FOLDER, "Elaris");
assert_eq!(LEGACY_DATA_FOLDER, "Photon");
```

The crate has no dependencies so every Photon crate can use it. New internal links use `elaris://`; old `photon://` links remain supported. Storage migrates the profile and settings folders from `Photon` to `Elaris` when the destination is not already present. The planet logo remains in use in the shell UI and in the operating-system icon resources.
