// SPDX-License-Identifier: GPL-3.0-only

//! Interface icons, one module per icon. Each is a 16-point SVG body drawn in `currentColor`, so it takes the
//! colour of the text around it. The search providers' brand marks and the confirmation tick are separate modules.

use rune::Icon;

mod activity;
mod arrow_down;
mod autoplay;
mod back;
mod bed;
mod camera;
mod chevron_down;
mod chevron_right;
mod chevron_up;
mod close;
mod contrast;
mod copy;
mod dark;
mod download;
mod drop;
mod duplicate;
mod folder;
mod forward;
mod glass;
mod grain;
mod history;
mod info;
mod light;
mod link;
mod loading;
mod location;
mod lock;
#[cfg(elaris_ui = "linux")]
mod maximize;
mod microphone;
#[cfg(elaris_ui = "linux")]
mod minimize;
mod more;
mod notifications;
mod open;
mod open_tab;
mod page;
mod pause;
mod plus;
mod reload;
mod resume;
mod search;
mod sidebar;
mod site_controls;
mod site_popover_arrow;
mod split_view;
mod system;

mod search_engines;
mod tick;
mod translate;

pub(crate) use activity::ACTIVITY;
pub(crate) use arrow_down::ARROW_DOWN;
pub(crate) use autoplay::AUTOPLAY;
pub(crate) use back::BACK;
pub(crate) use bed::BED;
pub(crate) use camera::CAMERA;
pub(crate) use chevron_down::CHEVRON_DOWN;
pub(crate) use chevron_right::CHEVRON_RIGHT;
pub(crate) use chevron_up::CHEVRON_UP;
pub(crate) use close::CLOSE;
pub(crate) use contrast::CONTRAST;
pub(crate) use copy::COPY;
pub(crate) use dark::DARK;
pub(crate) use download::DOWNLOAD;
pub(crate) use drop::DROP;
pub(crate) use duplicate::DUPLICATE;
pub(crate) use folder::FOLDER;
pub(crate) use forward::FORWARD;
pub(crate) use glass::GLASS;
pub(crate) use grain::GRAIN;
pub(crate) use history::HISTORY;
pub(crate) use info::INFO;
pub(crate) use light::LIGHT;
pub(crate) use link::LINK;
pub(crate) use loading::LOADING;
pub(crate) use location::LOCATION;
pub(crate) use lock::LOCK;
#[cfg(elaris_ui = "linux")]
pub(crate) use maximize::MAXIMIZE;
pub(crate) use microphone::MICROPHONE;
#[cfg(elaris_ui = "linux")]
pub(crate) use minimize::MINIMIZE;
pub(crate) use more::MORE;
pub(crate) use notifications::NOTIFICATIONS;
pub(crate) use open::OPEN;
pub(crate) use open_tab::OPEN_TAB;
pub(crate) use page::PAGE;
pub(crate) use pause::PAUSE;
pub(crate) use plus::PLUS;
pub(crate) use reload::RELOAD;
pub(crate) use resume::RESUME;
pub(crate) use search::SEARCH;
pub(crate) use search_engines::search_engine;
pub(crate) use sidebar::SIDEBAR;
pub(crate) use site_controls::SITE_CONTROLS;
pub(crate) use site_popover_arrow::SITE_POPOVER_ARROW;
pub(crate) use split_view::SPLIT_VIEW;
pub(crate) use system::SYSTEM;
pub(crate) use tick::TICK;
pub(crate) use translate::TRANSLATE;

const fn icon(name: &'static str, body: &'static str) -> Icon {
    Icon {
        name,
        view: 16.0,
        body,
    }
}
