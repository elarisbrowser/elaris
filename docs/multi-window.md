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

## Manual desktop checks

- Use Cmd+N on macOS or Ctrl+N on Linux to open two windows; navigate each to a different site.
- Switch tabs, reorder tabs, use Back/Forward and reload in both windows, and verify the other
  window stays on its own page.
- Open the site controls popover and native File menu in each window; close one window and repeat
  in the remaining window.
- On macOS, close the final window, activate Elaris from the Dock, then quit with Cmd+Q.
- Quit with multiple windows open and launch again to verify each window and its active tab restore.
