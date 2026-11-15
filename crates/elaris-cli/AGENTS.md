# elaris-cli

Keep one module per command and reuse `support.rs` for process execution,
diagnostics, progress, and terminal output. Formatting and Rust checks operate
on the complete Cargo workspace; do not add package-name lists that omit new
crates.

`commands/install.rs` installs or refreshes only the Cargo CLI binaries. Keep
Linux XDG desktop entry and URL-handler management in
`commands/linux_desktop.rs`, exposed as `elaris desktop install` and
`elaris desktop uninstall` for manual maintenance. Full `elaris setup` invokes
desktop installation automatically after the CLI is installed;
`elaris setup --ui-only` must leave existing desktop files untouched and must
not install them.
