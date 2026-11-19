#pragma once

#include <LibWebView/Export.h>
#include <LibWebView/ViewImplementation.h>

namespace Elaris {

// The link bounds are relative to the click in CSS pixels. The host supplies the click's position
// in its own window, even when the link is in a frame.
using PeekLinkHandler = void (*)(WebView::ViewImplementation const&, Web::HTML::PreparedNavigationDescriptor,
                                 ByteString const& bounds);

WEBVIEW_API void set_peek_link_handler(PeekLinkHandler);

} // namespace Elaris
