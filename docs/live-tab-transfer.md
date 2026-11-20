# Live tab ownership transfer

The engine thread owns Ladybird views in a map keyed by global tab ID. The native bridge delivers callbacks using that identity, and each Browser drains only the updates for its current tabs. A tab carries the shared Rune pixel stream that the engine writes into. Rune draws that stream in the owning window's viewport. Moving a tab between Browser instances on the same engine therefore preserves the view, compositor, and queued updates. The destination's normal viewport synchronization applies its dimensions, scale, visibility, and focus to the existing view.

The browser transfer validates both owners and the destination section index before removing the tab. It retains the tab ID and surface, releases source page focus, and activates the moved tab in the destination. Source replacement selection uses the same logic as closing a tab. A failed validation leaves both browsers unchanged.

## Window integration still needed

Rune revision `9553c9a` exposes drag positions in the originating window and provides window placement in physical pixels on macOS and X11. Its placement API explicitly returns no coordinates on Wayland. Placement alone also cannot identify the topmost eligible window when windows overlap. A complete gesture needs platform-aware hit testing and a preview that can follow the pointer outside the source surface. Wayland needs compositor-mediated drag handling or a reduced feature set.

Each Rune window holds its own AppState. AppHandle updates run through separate UI-thread queues, including while windows are hidden. A window-level transfer needs a commit protocol that handles destination closure and native window creation failure and saves both final session records together. The Browser primitive currently takes both owners directly. There is no sidebar gesture bound to it yet.
