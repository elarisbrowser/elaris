# Elaris Browser

Elaris is a small cross-platform browser shell built with Rust and [Rune](https://github.com/anomalyco/rune). The current app opens a dark native window with an editable address bar and an empty content area. Web rendering and Ladybird integration are not included yet.

## Build and run

Install the stable Rust toolchain, then run:

```sh
cargo run -p elaris-app
```

Rune supports macOS and Linux. On Linux, a working desktop session and GPU driver are required to open the native window.
