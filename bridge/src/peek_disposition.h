#pragma once

#include <LibWebCommon/HTML/ActivateTab.h>

namespace Elaris {

// Private to Elaris's build of LibWebView. This value never crosses Ladybird IPC.
inline constexpr auto peek_disposition = static_cast<Web::HTML::ActivateTab>(2);

} // namespace Elaris
