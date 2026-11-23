# Installing Elaris

Elaris is experimental alpha software. Expect rough edges and breaking changes. Each release is a draft pre-release on
GitHub until it has been checked, so download only from a published release.

Check each download against its `.sha256` file before you run it.

## macOS (Apple Silicon)

Requirements: macOS 14.4 or newer, on an Apple Silicon Mac. Intel Macs are not supported for the alpha.

1. Open `Elaris-<version>-macos-arm64.dmg`.
2. Drag **Elaris** into **Applications**.
3. Open Elaris from Applications. The alpha build is not notarized, so macOS will refuse the first launch. Open
   **System Settings → Privacy & Security**, find the message about Elaris, and choose **Open Anyway**.

## Linux (x86_64)

Requirements: a 64-bit x86 Linux desktop with a Vulkan driver (the system's GPU driver, or Mesa). Other architectures are
not built.

1. Extract the archive into your home directory:

   ```sh
   tar -xzf Elaris-<version>-linux-x86_64.tar.gz -C ~/.local/share
   ```

2. Link the launcher and command into your local prefix:

   ```sh
   mkdir -p ~/.local/bin
   ln -sf ~/.local/share/Elaris/bin/elaris ~/.local/bin/elaris
   cp -R ~/.local/share/Elaris/share/. ~/.local/share/
   update-desktop-database ~/.local/share/applications
   ```

   Make sure `~/.local/bin` is on your `PATH`. Elaris then appears in your application menu.

3. Run `elaris`, or `elaris https://example.com` to open a page.

The archive bundles Elaris's own libraries and Ladybird's engine. It uses the system's C library, C++ runtime, and
graphics stack.
