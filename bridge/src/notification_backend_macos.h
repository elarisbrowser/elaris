// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <LibWebView/NotificationBackend.h>

namespace Elaris {

// Notification Center delivery for Ladybird notifications. Elaris runs Ladybird on its own thread, so every
// Notification Center reply is moved back onto that thread's event loop instead of the main queue.
NonnullOwnPtr<WebView::NotificationBackend> make_notification_backend_macos();

} // namespace Elaris
