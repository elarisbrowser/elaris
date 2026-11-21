// SPDX-License-Identifier: GPL-3.0-only

#pragma once

#include <LibWebView/SpellChecker.h>

namespace Elaris {

// Checks spelling with the system's NSSpellChecker, so Elaris shares the user's dictionaries with other apps.
NonnullOwnPtr<WebView::SpellChecker> make_spell_checker_macos();

} // namespace Elaris
