# Live tab ownership transfer

The engine thread owns Ladybird views in a map keyed by global tab ID. The native bridge delivers callbacks using that identity, and each Browser drains only the updates for its current tabs. A tab carries the shared Rune pixel stream that the engine writes into. Rune draws that stream in the owning window's viewport. Moving a tab between Browser instances on the same engine therefore preserves the view, compositor, and queued updates. The destination's normal viewport synchronization applies its dimensions, scale, visibility, and focus to the existing view.

The browser transfer validates both owners and the destination section index before removing the tab. It retains the tab ID and surface, releases source page focus, and activates the moved tab in the destination. Source replacement selection uses the same logic as closing a tab. A failed validation leaves both browsers unchanged.

## Window integration still needed

Rune revision `9553c9a` exposes drag positions in the originating window and provides window placement in physical pixels on macOS and X11. Its placement API explicitly returns no coordinates on Wayland. Placement alone also cannot identify the topmost eligible window when windows overlap. A complete gesture needs platform-aware hit testing and a preview that can follow the pointer outside the source surface. Wayland needs compositor-mediated drag handling or a reduced feature set.

## Existing-window coordination

A source window can request a handoff through `AppState::request_tab_transfer`.
The destination validates the index and engine and reserves space during its own
Rune poll. The source then moves the live tab into the coordinator, and the
destination revalidates before claiming it. Escape or window closure cancels a
pending handoff. If extraction already happened, the source restores the tab
before its next normal session save or window teardown.

Once extracted, both participants suspend their routine session saves. The
registry retains the previous complete layout until the destination claims the
tab. At commit, the coordinator replaces both window records and sends a single
snapshot to the existing atomic session writer. The source acknowledges that
snapshot in its next poll before routine saving resumes. A crash before the
snapshot is written can restore the previous layout.

The coordinator currently processes one handoff at a time. Drag hit testing,
insertion previews, cross-window animations, detachment, and closing an emptied
source are separate stages.
