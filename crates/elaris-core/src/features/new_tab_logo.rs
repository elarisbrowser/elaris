//! The built-in new tab logo feature.

use super::*;

pub(super) const DEFINITION: FeatureDefinition = feature!(
        NewTabLogo,
        "new-tab-logo",
        "New tab logo",
        "Show the Elaris logo on the new tab page.",
        NewTab,
        NewTabLogo,
        NO_REQUIREMENTS
    );
