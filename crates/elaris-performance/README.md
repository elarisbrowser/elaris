<p align="center"><img src="../elaris-brand/assets/logo.svg" alt="Photon" width="72" /></p>

# `elaris-performance`

Framework-independent performance data and diagnostics: thread-safe frame and engine statistics, bounded presentation timing history, process sampling, and formatting. GPUI charts and overlays live in [`elaris-performance-ui`](../elaris-performance-ui/README.md).

## Use

Collect a lightweight view snapshot through `PerformanceMonitor`:

```rust
use elaris_performance::PerformanceMonitor;

let monitor = PerformanceMonitor::default();
monitor.set_enabled(true);
monitor.mark_input();
monitor.record_frame();
let snapshot = monitor.snapshot();
```

The shell supplies Engine statistics and semantic colors. Keep Engine IPC and native embedder details in [`elaris-shell`](../elaris-shell/README.md).
