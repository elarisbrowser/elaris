// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <LibURL/URL.h>
#include <LibWebView/ExternalURLHandler.h>

namespace Elaris {

// Finds the application the system would open `url` with. The callback, and any launch completion,
// run on the calling thread's event loop, which for Elaris is the Ladybird owner thread.
void resolve_external_url_handler(URL::URL const& url, WebView::ExternalURLHandlerCallback callback);

} // namespace Elaris
