# Installing Elaris

Elaris is experimental alpha software. Download from a published release or the private link
provided for your testing cohort. If you want to build from source, see [development setup](../SETUP.md).

Before opening the package, verify it against the matching `.sha256` file. Replace `your-package.sha256`
with the checksum filename and run the command in the directory containing both files:

- macOS: `shasum -a 256 -c your-package.sha256`
- Linux: `sha256sum -c your-package.sha256`

## macOS (Apple Silicon)

Requirements: macOS 14.4 or newer, on an Apple Silicon Mac. Intel Macs are not supported for the alpha.

1. Open `Elaris-<version>-macos-arm64.dmg`.
2. Drag **Elaris** into **Applications**.
3. Open Elaris from Applications. On a signed and notarized build, macOS may ask you to confirm the first launch.
   If macOS blocks a build that was distributed as signed, stop and report the warning and package filename.
   Older unsigned test builds do not use [the macOS signing workflow](macos-signing.md).

## Linux (x86_64)

Requirements: a 64-bit x86 Linux desktop with glibc 2.39 or newer and a Vulkan driver (the system's GPU driver or Mesa).
Other architectures are not built.

### AppImage

Download `Elaris-<version>-linux-x86_64.AppImage`, make it executable, then run it:

```sh
chmod +x Elaris-<version>-linux-x86_64.AppImage
./Elaris-<version>-linux-x86_64.AppImage
```

The AppImage includes the browser, engine, and bundled libraries. If FUSE 2 is missing, launch
the same file with `APPIMAGE_EXTRACT_AND_RUN=1` set, or use the tarball below.

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

The archive still uses the system's C library, C++ runtime and graphics stack. Your profile stays in
`${XDG_DATA_HOME:-$HOME/.local/share}/Elaris`, separate from the install. Leave that directory in
place when upgrading or moving the browser.
