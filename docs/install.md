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

Requirements: a 64-bit x86 Linux desktop with glibc 2.43 or newer and a Vulkan driver (the system's GPU driver or Mesa).
Ubuntu 24.04 does not meet the glibc requirement. Other architectures are not built.

### AppImage

Download `Elaris-<version>-linux-x86_64.AppImage`, make it executable, then run it:

```sh
chmod +x Elaris-<version>-linux-x86_64.AppImage
./Elaris-<version>-linux-x86_64.AppImage
```

The AppImage includes Elaris, Ladybird's helper processes and resources, and the bundled libraries. It still requires
glibc 2.43 or newer and a Vulkan driver. Some distributions may also need FUSE 2; set `APPIMAGE_EXTRACT_AND_RUN=1`
to run it without FUSE.

### Tarball

1. Extract the archive into a separate application directory, outside your browser profile. Replace `<version>` with the
   version in the filename.

   ```sh
   install_root="$HOME/.local/opt"
   mkdir -p "$install_root"
   tar -xzf "Elaris-<version>-linux-x86_64.tar.gz" -C "$install_root"
   ```

2. Launch the browser:

   ```sh
   "$HOME/.local/opt/Elaris/bin/elaris"
   ```

3. Optional: add the terminal command and desktop entry.

   ```sh
   install_dir="$HOME/.local/opt/Elaris"
   data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
   mkdir -p "$HOME/.local/bin" "$data_home/applications" "$data_home/icons"
   ln -sf "$install_dir/bin/elaris" "$HOME/.local/bin/elaris"
   cp "$install_dir/share/applications/org.elarisbrowser.Elaris.desktop" "$data_home/applications/"
   cp -R "$install_dir/share/icons/." "$data_home/icons/"
   ```

   The desktop entry runs `elaris` from your desktop session's `PATH`. Make sure that `$HOME/.local/bin` is
   on that path before launching from the application menu. If it isn't, use the direct command in step 2.

The archive bundles Elaris's own libraries and Ladybird's engine. It uses the system's C library, C++ runtime, and
graphics stack. Your profile stays in `${XDG_DATA_HOME:-$HOME/.local/share}/Elaris`, separate from the install.
If you used the previous instructions, leave that data directory in place when installing to the new location.
