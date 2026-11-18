# Downloads

Elaris uses the pinned Ladybird `LibWebView::FileDownloader` for page initiated and direct HTTP(S) downloads. The bridge does not implement HTTP, write response bodies, or maintain a second transfer engine.

## Ladybird integration

The vendored Ladybird revision is pinned by the `vendor/ladybird` submodule. Its `WebContentPage` reports a download URL, response suggested filename, optional content length, and any response bytes already received. `Application` routes the request to `FileDownloader`, which owns the network transfer, progress, temporary `.download` file, final rename, collision protection, cancellation, and transfer errors. The Qt front end observes these downloads and supplies open, reveal, and cancel actions.

Elaris's C++ bridge observes `FileDownloader` and copies URL, filename, destination, status, byte counts, and errors into the C callback for the duration of that call. Rust copies those values into owned `Download` snapshots. The browser maps Ladybird's private numeric IDs to stable `DownloadId`s. Page initiated transfers belong to Ladybird's application, so they keep running when their source tab closes.

Ladybird's application uses the platform Downloads directory from `Core::StandardPaths`; its filename utilities sanitize suggested names and add collision suffixes. Elaris leaves that behavior intact by default. No Save As dialog is needed for this milestone: the bridge selects the default Downloads destination directly. `ELARIS_DOWNLOAD_DIR` can set an absolute destination for an Elaris process, and the low-level Rust bridge also has a per-instance directory override for isolated fixtures.

## Rust API and UI

`elaris::downloads` contains `DownloadId`, `Request`, `Download`, and `Status`, without Rune or C++ types. The browser owns the visible snapshots. Direct starts, cancel, open, and reveal are queued to the engine thread; Ladybird transfers and filesystem activity stay off Rune's UI thread. Ladybird throttles transfer notifications, which also limits progress redraws.

The compact Downloads and appearance icon buttons share the sidebar footer, aligned with the page container's bottom inset. An active-count badge appears when transfers start. The Downloads popover lists current-process downloads, showing a determinate bar when the response length is known and an animated indeterminate bar otherwise, alongside transferred bytes. It can cancel active transfers, open completed files with the OS default handler, or reveal the file in Finder/the Linux file manager. The app does not execute downloaded files. Completed metadata is currently in memory only; Elaris does not restore or resume downloads after restart. On startup, any incomplete records restored by Ladybird are cancelled so they are not silently resumed.

## Fixture coverage

The C++ bridge smoke test serves local HTTP responses for a small file, streamed files, missing content length, attachment and link suggested filenames, duplicate names, a 404, cancellation, closing the source tab, and shutdown. It uses a build-directory profile and Downloads directory and refuses to clear a root inside the user's Downloads folder. `examples/ladybird_smoke.rs` exercises the Rust FFI callback, sanitized destination and cancellation using a second local fixture and an isolated profile.

For manual graphical checks, set `ELARIS_DOWNLOAD_DIR` to a disposable absolute directory before launching Elaris. This keeps verification downloads out of the user's actual Downloads folder.
