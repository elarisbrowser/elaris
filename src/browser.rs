// SPDX-License-Identifier: GPL-3.0-only

//! The browser: its tabs, which one is active, and the one engine behind them all.
//!
//! ```ignore
//! let tab = browser.open_tab("https://example.com")?;
//! browser.activate_tab(tab);
//! browser.navigate(tab, "https://ladybird.org")?;
//! browser.reload(tab);
//! browser.close_tab(tab);
//! ```
//!
//! Every operation returns at once. What it asks of the engine happens on the engine's thread,
//! and its outcome (the committed address, title, loading state, errors) arrives in the tab's
//! state through [`Browser::poll`]. Tabs are named by [`TabId`]s, which are never reused: an
//! operation on a tab that has closed does nothing, and nothing the engine reports about a closed
//! tab reaches another one. Only the active tab is visible, laid out at the viewport's size and
//! given focus and input; the others keep their pages but stop painting.

use std::cell::Cell;
use std::fmt;
use std::time::Instant;

use elaris::ladybird::{MouseEvent, MouseKind, WheelPhase};
use rune::KeyEvent;

use crate::address::url_for_address;
use crate::engine::{Command, Engine, Update, ViewCommand};
use crate::input;
use crate::perf::{self, Stage};
use crate::tab::{Tab, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Error {
    /// The address cannot be loaded, for the reason given.
    Address(&'static str),
    /// No open tab has this id.
    NoSuchTab(TabId),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(reason) => f.write_str(reason),
            Self::NoSuchTab(_) => f.write_str("The tab has closed"),
        }
    }
}

impl std::error::Error for Error {}

/// What one poll found.
#[derive(Default)]
pub(crate) struct Polled {
    /// Some tab's state changed, so the interface needs building again.
    pub(crate) changed: bool,
    /// The active tab showed a frame since the last poll, so it is likely animating.
    pub(crate) animating: bool,
}

pub(crate) struct Browser {
    /// In the order the sidebar shows them.
    tabs: Vec<Tab>,
    active: Option<TabId>,
    next_id: u64,
    /// None until the app connects one, and in a build without an engine.
    engine: Option<Engine>,
    /// Why there is no engine, which every tab shows.
    engine_error: Option<String>,
    window: Window,
    /// Development timing: the last poll.
    last_poll: Option<Instant>,
}

/// What the window tells the browser about the page viewport, which every tab shares.
struct Window {
    /// The display scale, as the view last saw it.
    scale: Cell<f32>,
    /// Whether the interface is dark, as the view last drew it, and what the engine was told.
    /// Unknown until the view first builds, and nothing is sent until then.
    dark: Cell<Option<bool>>,
    applied_dark: Cell<Option<bool>>,
    /// The viewport's size in points, from layout.
    viewport: [f32; 2],
    /// Whether the window, and the viewport within it, have focus.
    focused: bool,
    viewport_focused: bool,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            tabs: Vec::new(),
            active: None,
            next_id: 1,
            engine: None,
            engine_error: None,
            window: Window {
                scale: Cell::new(1.0),
                dark: Cell::new(None),
                applied_dark: Cell::new(None),
                viewport: [0.0; 2],
                focused: true,
                viewport_focused: false,
            },
            last_poll: None,
        }
    }
}

impl Browser {
    /// Gives the browser the engine that shows its tabs, opening a view for each.
    pub(crate) fn connect(&mut self, engine: Engine) {
        for tab in &self.tabs {
            engine.send(Command::Open(tab.id(), tab.surface.clone()));
        }
        self.engine = Some(engine);
    }

    /// Records that the engine cannot run, which every tab shows.
    pub(crate) fn fail(&mut self, error: String) {
        for tab in &mut self.tabs {
            tab.error = Some(error.clone());
        }
        self.engine_error = Some(error);
    }

    pub(crate) fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub(crate) fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id() == id)
    }

    pub(crate) fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id))
    }

    /// Opens a tab at the end of the list loading `address`, and makes it active if no tab is.
    pub(crate) fn open_tab(&mut self, address: &str) -> Result<TabId, Error> {
        let url = url_for_address(address).map_err(Error::Address)?;
        let id = TabId::from_raw(self.next_id);
        self.next_id += 1;
        let mut tab = Tab::new(id, url);
        tab.error.clone_from(&self.engine_error);
        if let Some(engine) = &self.engine {
            engine.send(Command::Open(id, tab.surface.clone()));
        }
        self.tabs.push(tab);
        self.active.get_or_insert(id);
        Ok(id)
    }

    /// Shows the tab in the viewport and gives it the page's input.
    pub(crate) fn activate_tab(&mut self, id: TabId) {
        if self.tab(id).is_some() {
            self.active = Some(id);
        }
    }

    /// Loads `address` in the tab.
    pub(crate) fn navigate(&mut self, id: TabId, address: &str) -> Result<(), Error> {
        let url = url_for_address(address).map_err(Error::Address)?;
        let connected = self.engine.is_some();
        let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
        tab.url.clone_from(&url);
        tab.error = None;
        // A tab the engine has not laid out yet loads once it has.
        if !connected || tab.engine.pending_load.is_some() {
            tab.engine.pending_load = Some(url);
        } else {
            self.send(id, ViewCommand::Load(url));
        }
        Ok(())
    }

    pub(crate) fn reload(&mut self, id: TabId) {
        self.send(id, ViewCommand::Reload);
    }

    pub(crate) fn go_back(&mut self, id: TabId) {
        self.send(id, ViewCommand::Traverse(-1));
    }

    pub(crate) fn go_forward(&mut self, id: TabId) {
        self.send(id, ViewCommand::Traverse(1));
    }

    /// Closes the tab and its page. Closing the active tab activates the one after it, or the
    /// one before when it was last.
    pub(crate) fn close_tab(&mut self, id: TabId) {
        let Some(index) = self.tabs.iter().position(|tab| tab.id() == id) else {
            return;
        };
        self.tabs.remove(index);
        if let Some(engine) = &self.engine {
            engine.send(Command::Close(id));
        }
        if self.active == Some(id) {
            self.active = self
                .tabs
                .get(index)
                .or_else(|| self.tabs.last())
                .map(Tab::id);
        }
    }

    /// Records the display scale. The view calls it as it builds, because only the view knows it.
    pub(crate) fn note_scale(&self, scale: f32) {
        self.window.scale.set(scale);
    }

    /// Records whether the interface is dark, so pages follow it. The view calls it as it
    /// builds, because only the view knows the system's appearance; a change goes to the engine
    /// at once, since no poll may follow this build.
    pub(crate) fn note_dark(&self, dark: bool) {
        self.window.dark.set(Some(dark));
        self.send_color_scheme();
    }

    pub(crate) fn set_viewport(&mut self, size: [f32; 2]) {
        self.window.viewport = size;
    }

    pub(crate) fn set_window_focus(&mut self, focused: bool) {
        self.window.focused = focused;
    }

    pub(crate) fn set_viewport_focus(&mut self, focused: bool) {
        self.window.viewport_focused = focused;
    }

    pub(crate) fn pointer(&mut self, kind: MouseKind, point: [f32; 2], held: bool, clicks: u32) {
        let event = input::pointer(kind, point, self.window.scale.get(), held, clicks);
        self.send_mouse(event);
    }

    pub(crate) fn wheel(
        &mut self,
        point: [f32; 2],
        delta: [f32; 2],
        precise: bool,
        phase: WheelPhase,
    ) {
        let event = input::wheel(point, delta, precise, phase, self.window.scale.get());
        self.send_mouse(event);
    }

    /// Sends a key press to the active tab's page; false when the page takes no such key.
    pub(crate) fn key(&mut self, event: &KeyEvent) -> bool {
        let Some(id) = self.active.filter(|_| self.engine.is_some()) else {
            return false;
        };
        let keys = input::keys(event);
        for key in &keys {
            self.send(id, ViewCommand::Key(*key));
        }
        !keys.is_empty()
    }

    fn send_mouse(&mut self, event: MouseEvent) {
        if let Some(id) = self.active {
            self.send(id, ViewCommand::Mouse(event));
        }
    }

    /// Forwards what changed to the engine and takes in what it reported. Rune calls it at the
    /// start of every frame; it never blocks.
    pub(crate) fn poll(&mut self) -> Polled {
        if perf::enabled() {
            let now = Instant::now();
            if let Some(last) = self.last_poll.replace(now) {
                perf::record(Stage::Poll, now - last);
            }
        }
        let mut polled = Polled::default();
        self.sync_engine();
        // Collected first: taking them in needs the browser, which the engine is part of.
        // Empty, as most polls are, it allocates nothing.
        let updates: Vec<Update> = self.engine.iter().flat_map(Engine::updates).collect();
        for update in updates {
            match update {
                // A tab closed since the engine reported this has nothing to show it.
                Update::Tab(id, update) => {
                    if let Some(tab) = self.tab_mut(id) {
                        tab.update(update);
                        polled.changed = true;
                    }
                }
                Update::Failed(error) => {
                    self.fail(error);
                    polled.changed = true;
                }
            }
        }
        if let Some(id) = self.active
            && let Some(tab) = self.tab_mut(id)
        {
            let frames = tab.surface.frames();
            polled.animating = std::mem::replace(&mut tab.engine.frames, frames) != frames;
        }
        polled
    }

    /// Asks the engine to close every page and stop, without waiting for it.
    pub(crate) fn shut_down(&mut self) {
        if let Some(engine) = &self.engine {
            engine.shut_down();
        }
    }

    fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id() == id)
    }

    /// Sends `command` to the tab's view, if the tab is open and there is an engine.
    fn send(&self, id: TabId, command: ViewCommand) {
        if let Some(engine) = &self.engine
            && self.tab(id).is_some()
        {
            engine.send(Command::View(id, command));
        }
    }

    fn send_color_scheme(&self) {
        if let Some(engine) = &self.engine
            && let Some(dark) = self.window.dark.get()
            && self.window.applied_dark.replace(Some(dark)) != Some(dark)
        {
            engine.send(Command::ColorScheme(dark));
        }
    }

    /// Tells the engine what changed about each tab: whether it is shown and focused, the
    /// viewport it lays out in, and its first page.
    fn sync_engine(&mut self) {
        // Before any first load, so the first page lays out in the right scheme.
        self.send_color_scheme();
        let Some(engine) = &self.engine else {
            return;
        };
        let window = &self.window;
        let scale = window.scale.get();
        let [width, height] = window.viewport.map(|points| input::device(points, scale));
        let viewport = (width > 0 && height > 0).then_some((width, height, f64::from(scale)));
        let scheme_known = window.applied_dark.get().is_some();
        for tab in &mut self.tabs {
            let id = tab.id();
            let applied = &mut tab.engine;
            let active = self.active == Some(id);
            let send = |command| engine.send(Command::View(id, command));
            if applied.hidden == active {
                applied.hidden = !active;
                send(ViewCommand::Visible(active));
            }
            // Hidden tabs keep their last layout, unless they have yet to load their first page.
            if (active || applied.pending_load.is_some())
                && let Some(wanted) = viewport
                && applied.viewport != Some(wanted)
            {
                let (width, height, scale) = wanted;
                send(ViewCommand::Resize {
                    width,
                    height,
                    scale,
                });
                applied.viewport = Some(wanted);
            }
            // The first load waits for a viewport and the colour scheme, so the page lays out
            // at its real size and never starts in the wrong scheme.
            if applied.viewport.is_some()
                && scheme_known
                && let Some(url) = applied.pending_load.take()
            {
                send(ViewCommand::Load(url));
            }
            let focused = active && window.focused && window.viewport_focused;
            if applied.focused != focused {
                applied.focused = focused;
                send(ViewCommand::Focus(focused));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use rune::{Key as RuneKey, Mods};

    use super::*;
    use crate::engine::{FakeEngine, Surface, TabUpdate};

    fn connected() -> (Browser, FakeEngine) {
        let mut browser = Browser::default();
        let (engine, fake) = Engine::fake();
        browser.connect(engine);
        (browser, fake)
    }

    fn sent(fake: &FakeEngine) -> Vec<Command> {
        fake.commands.try_iter().collect()
    }

    /// What the view does as it builds and lays out.
    fn lay_out(browser: &mut Browser, size: [f32; 2], scale: f32, dark: bool) {
        browser.note_scale(scale);
        browser.note_dark(dark);
        browser.set_viewport(size);
    }

    /// A browser laid out at 100x50 points, scale 1, with its commands so far read.
    fn ready() -> (Browser, FakeEngine) {
        let (mut browser, fake) = connected();
        lay_out(&mut browser, [100.0, 50.0], 1.0, false);
        browser.poll();
        sent(&fake);
        (browser, fake)
    }

    fn view(id: TabId, command: ViewCommand) -> Command {
        Command::View(id, command)
    }

    fn resize(width: i32, height: i32, scale: f64) -> ViewCommand {
        ViewCommand::Resize {
            width,
            height,
            scale,
        }
    }

    fn surface(browser: &Browser, id: TabId) -> Surface {
        browser.tab(id).unwrap().surface.clone()
    }

    #[test]
    fn a_tab_opened_before_the_engine_opens_once_it_connects() {
        let mut browser = Browser::default();
        let tab = browser.open_tab("example.com").unwrap();
        let (engine, fake) = Engine::fake();
        browser.connect(engine);
        assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);
        assert_eq!(browser.active_tab().map(Tab::id), Some(tab));
    }

    #[test]
    fn the_first_page_waits_for_its_viewport_and_colour_scheme() {
        let (mut browser, fake) = connected();
        let tab = browser.open_tab("about:blank").unwrap();
        browser.poll();
        assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);

        lay_out(&mut browser, [100.0, 50.0], 2.0, true);
        browser.poll();
        assert_eq!(
            sent(&fake),
            [
                // Building sends the scheme at once, ahead of the poll's commands.
                Command::ColorScheme(true),
                view(tab, resize(200, 100, 2.0)),
                view(tab, ViewCommand::Load("about:blank".into())),
            ]
        );
        browser.poll();
        assert_eq!(sent(&fake), []);
    }

    #[test]
    fn only_the_active_tab_is_shown_resized_and_focused() {
        let (mut browser, fake) = ready();
        browser.set_viewport_focus(true);
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        browser.poll();
        assert_eq!(
            sent(&fake),
            [
                Command::Open(first, surface(&browser, first)),
                Command::Open(second, surface(&browser, second)),
                view(first, resize(100, 50, 1.0)),
                view(first, ViewCommand::Load("https://example.com".into())),
                view(first, ViewCommand::Focus(true)),
                // A background tab lays out once, to load its first page, then hides.
                view(second, ViewCommand::Visible(false)),
                view(second, resize(100, 50, 1.0)),
                view(second, ViewCommand::Load("https://example.org".into())),
            ]
        );

        // A window resize reaches only the tab on show.
        browser.set_viewport([120.0, 50.0]);
        browser.poll();
        assert_eq!(sent(&fake), [view(first, resize(120, 50, 1.0))]);

        browser.activate_tab(second);
        browser.poll();
        assert_eq!(
            sent(&fake),
            [
                view(first, ViewCommand::Visible(false)),
                view(first, ViewCommand::Focus(false)),
                view(second, ViewCommand::Visible(true)),
                view(second, resize(120, 50, 1.0)),
                view(second, ViewCommand::Focus(true)),
            ]
        );
    }

    #[test]
    fn input_goes_only_to_the_active_tab_in_device_pixels() {
        let (mut browser, fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        browser.open_tab("example.org").unwrap();
        browser.poll();
        sent(&fake);

        browser.note_scale(2.0);
        browser.pointer(MouseKind::Down, [1.0, 2.0], true, 1);
        let enter = KeyEvent {
            key: RuneKey::Enter,
            mods: Mods::default(),
        };
        assert!(browser.key(&enter));
        let shortcut = KeyEvent {
            key: RuneKey::Command('q'),
            mods: Mods::default(),
        };
        assert!(!browser.key(&shortcut));
        let sent = sent(&fake);
        assert_eq!(sent.len(), 3);
        assert!(matches!(
            sent[0],
            Command::View(id, ViewCommand::Mouse(MouseEvent { x: 2, y: 4, .. })) if id == first
        ));
        assert!(
            sent[1..]
                .iter()
                .all(|c| matches!(c, Command::View(id, ViewCommand::Key(_)) if *id == first))
        );
    }

    #[test]
    fn navigation_and_history_go_to_the_tab_named() {
        let (mut browser, fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        browser.poll();
        sent(&fake);

        browser.navigate(second, "ladybird.org").unwrap();
        browser.reload(first);
        browser.go_back(second);
        browser.go_forward(first);
        assert_eq!(
            sent(&fake),
            [
                view(second, ViewCommand::Load("https://ladybird.org".into())),
                view(first, ViewCommand::Reload),
                view(second, ViewCommand::Traverse(-1)),
                view(first, ViewCommand::Traverse(1)),
            ]
        );
        assert_eq!(browser.tab(second).unwrap().url, "https://ladybird.org");
        assert_eq!(browser.tab(first).unwrap().url, "https://example.com");
    }

    #[test]
    fn says_why_it_cannot_navigate() {
        let (mut browser, fake) = ready();
        let tab = browser.open_tab("example.com").unwrap();
        assert!(matches!(
            browser.open_tab("two words"),
            Err(Error::Address(_))
        ));
        assert!(matches!(
            browser.navigate(tab, "two words"),
            Err(Error::Address(_))
        ));
        browser.close_tab(tab);
        assert_eq!(
            browser.navigate(tab, "example.org"),
            Err(Error::NoSuchTab(tab))
        );
        assert_eq!(browser.tabs().len(), 0);
        // Only the one tab opened and closed; nothing else reached the engine.
        assert_eq!(
            sent(&fake).last(),
            Some(&Command::Close(tab)),
            "a closed tab takes no commands"
        );
    }

    #[test]
    fn closing_the_active_tab_activates_its_neighbour() {
        let (mut browser, _fake) = ready();
        let [a, b, c] = ["a.com", "b.com", "c.com"].map(|a| browser.open_tab(a).unwrap());
        browser.activate_tab(b);

        // An inactive tab closes without changing which one is active.
        browser.close_tab(a);
        assert_eq!(browser.active_tab().map(Tab::id), Some(b));

        // The active one hands over to the tab after it, or before it when it was last.
        browser.close_tab(b);
        assert_eq!(browser.active_tab().map(Tab::id), Some(c));
        let d = browser.open_tab("d.com").unwrap();
        browser.activate_tab(d);
        browser.close_tab(d);
        assert_eq!(browser.active_tab().map(Tab::id), Some(c));
        browser.close_tab(c);
        assert!(browser.active_tab().is_none());
    }

    #[test]
    fn tab_ids_are_never_reused() {
        let (mut browser, _fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        browser.close_tab(first);
        let second = browser.open_tab("example.com").unwrap();
        assert_ne!(first, second);
        assert!(browser.tab(first).is_none());
        browser.activate_tab(first);
        assert_eq!(browser.active_tab().map(Tab::id), Some(second));
    }

    #[test]
    fn updates_reach_only_their_own_tab_and_never_a_closed_one() {
        let (mut browser, fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        let send = |id, update| fake.updates.send(Update::Tab(id, update)).unwrap();
        send(first, TabUpdate::Title("First".into()));
        send(second, TabUpdate::Title("Second".into()));
        send(second, TabUpdate::Loading(true));
        assert!(browser.poll().changed);
        assert_eq!(browser.tab(first).unwrap().title, "First");
        assert!(!browser.tab(first).unwrap().loading);
        assert_eq!(browser.tab(second).unwrap().title, "Second");
        assert!(browser.tab(second).unwrap().loading);

        browser.close_tab(second);
        send(second, TabUpdate::Title("Late".into()));
        assert!(!browser.poll().changed);
        assert_eq!(browser.tab(first).unwrap().title, "First");
    }

    #[test]
    fn only_the_active_tabs_frames_mean_animation() {
        let (mut browser, _fake) = ready();
        let first = browser.open_tab("example.com").unwrap();
        let second = browser.open_tab("example.org").unwrap();
        assert!(!browser.poll().animating);
        surface(&browser, second).fake_frame();
        assert!(!browser.poll().animating);
        surface(&browser, first).fake_frame();
        assert!(browser.poll().animating);
        assert!(!browser.poll().animating);
    }

    #[test]
    fn an_engine_failure_shows_in_every_tab_and_new_ones() {
        let (mut browser, fake) = ready();
        let tab = browser.open_tab("example.com").unwrap();
        fake.updates.send(Update::Failed("gone".into())).unwrap();
        assert!(browser.poll().changed);
        assert_eq!(browser.tab(tab).unwrap().error.as_deref(), Some("gone"));
        let later = browser.open_tab("example.org").unwrap();
        assert_eq!(browser.tab(later).unwrap().error.as_deref(), Some("gone"));
    }

    #[test]
    fn without_an_engine_tabs_keep_their_page_and_take_no_keys() {
        let mut browser = Browser::default();
        let tab = browser.open_tab("example.com").unwrap();
        browser.navigate(tab, "example.org").unwrap();
        assert_eq!(
            browser.tab(tab).unwrap().engine.pending_load.as_deref(),
            Some("https://example.org")
        );
        let key = KeyEvent {
            key: RuneKey::Enter,
            mods: Mods::default(),
        };
        assert!(!browser.key(&key));
        assert!(!browser.poll().changed);
    }

    #[test]
    fn shutting_down_asks_the_engine_to_stop() {
        let (mut browser, fake) = connected();
        browser.shut_down();
        assert_eq!(sent(&fake), [Command::Shutdown]);
    }
}
