# Multi-window browser state

Each native browser window is a separate Rune `AppState` with its own `Browser`, tabs, active tab,
sidebar, focus, menus and popovers. Runtime window keys and tab IDs are unique for the process.
Ladybird remains process-wide: windows send commands through cloned handles to the one engine
thread, and engine updates are routed by tab ID. Closing a tab removes its ID from the live set so
late notifications are discarded.

Settings, site permissions, download records, profile storage and the window registry are shared
services. Session files are now a versioned collection of per-window snapshots. Existing single
window session files load as one-window collections. Closing one of several windows removes that
window from the collection. Closing the final window on macOS records its last snapshot but marks
it not to restore on a later launch; Dock activation creates a fresh window in the still-running
process. Quitting with windows open marks the collection for normal startup restoration.

## Mini windows

A mini window is an `AppState` flagged `mini`: one page under a slim header (navigation, the
site, and Open in Elaris) with no sidebar, omnibox or tab transfer. It is never part of the saved
session or window count, and closes with its last page.

- **Links from other apps.** Every window installs Rune's `on_open_url` hook. A plain `http` or
  `https` link opens a mini window; anything else is dropped. Rune delivers the link on macOS
  (the get-URL Apple event). On Linux a link launches a new process, which opens its
  page in a full window as before.
- **Popups.** `window.open` popups no longer become tabs. The opener's browser reports the pending
  popup, the app opens a mini window, and that window adopts the popup's view once its engine
  connection is up, so `window.opener` keeps working. Private windows open private popups.
  Headless tooling opens no windows, so popups stay tabs there.
- **Sizes.** `MINI_WINDOW_SIZE` and `MINI_WINDOW_MIN_SIZE` in `ui/theme.rs`.

## Manual desktop checks

- Run the bundled app, make it the default browser, and open a link from another app. It should
  open in a mini window; closing it leaves the other windows and session untouched.
- Open a page that calls `window.open` (an OAuth button). The popup should appear in a mini
  window and close itself when the page calls `window.close()`.

- Use Cmd+N on macOS or Ctrl+N on Linux to open two windows; navigate each to a different site.
- Switch tabs, reorder tabs, use Back/Forward and reload in both windows, and verify the other
  window stays on its own page.
- Open the site controls popover and native File menu in each window; close one window and repeat
  in the remaining window.
- On macOS, close the final window, activate Elaris from the Dock, then quit with Cmd+Q.
- Quit with multiple windows open and launch again to verify each window and its active tab restore.
