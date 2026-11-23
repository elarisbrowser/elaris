// SPDX-License-Identifier: GPL-3.0-only

//! Native macOS application menus, dispatched into Elaris's shared browser actions.

mod construct;
mod dispatch;
mod items;
mod quit;
mod state;
mod target;
mod window;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use objc2_05::rc::Retained;
use objc2_05::runtime::ProtocolObject;
use objc2_app_kit_02::{NSMenuDelegate, NSMenuItem};

use self::state::MenuState;
use crate::app::AppState;

pub(crate) use self::construct::install;
pub(crate) use self::quit::{quit_cancelled, quit_window_approved};
#[cfg_attr(elaris_ui = "linux", allow(unused_imports))]
pub(crate) use self::window::refresh;
pub(crate) use self::window::{register_window, set_active_window, unregister_window};

thread_local! {
    static MENU_ITEMS: RefCell<Vec<Retained<NSMenuItem>>> = const { RefCell::new(Vec::new()) };
    static MENU_TARGET: RefCell<Option<Retained<ProtocolObject<dyn NSMenuDelegate>>>> = const { RefCell::new(None) };
    static FULL_SCREEN_ITEM: RefCell<Option<Retained<NSMenuItem>>> = const { RefCell::new(None) };
    static MENU_STATE: RefCell<Option<MenuState>> = const { RefCell::new(None) };
    static APP_HANDLES: RefCell<HashMap<u64, rune::AppHandle<AppState>>> = RefCell::new(HashMap::new());
    static ACTIVE_WINDOW: Cell<Option<u64>> = const { Cell::new(None) };
    /// Windows whose pages have not yet agreed to unload for a pending quit.
    static QUIT_PENDING: RefCell<Option<HashSet<u64>>> = const { RefCell::new(None) };
}
