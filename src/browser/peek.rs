// SPDX-License-Identifier: GPL-3.0-only

//! Owns the temporary page and transfers its existing view into the tab strip on promotion.

use super::{Browser, Command, Tab, TabId, ViewCommand, split};

pub(crate) const OUTSET: f32 = 4.0;
pub(crate) const ACTION_RAIL: f32 = 40.0;

pub(crate) struct PeekState {
    pub(crate) page: Tab,
    /// Click position relative to the entire content region, in logical points.
    pub(crate) origin: [f32; 2],
}

#[derive(Clone, Copy)]
pub(crate) enum Destination {
    Tab,
    Split,
}

pub(crate) struct Geometry {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

impl Geometry {
    pub(crate) fn page_size(&self) -> [f32; 2] {
        [(self.width - ACTION_RAIL).max(0.0), self.height]
    }
}

pub(crate) fn geometry([width, height]: [f32; 2]) -> Geometry {
    let panel_width = width * 0.86;
    Geometry {
        x: width - panel_width,
        y: -OUTSET,
        width: panel_width,
        height: height + 2.0 * OUTSET,
    }
}

pub(crate) fn shift_only(mods: rune::Mods) -> bool {
    mods.shift && !(mods.cmd || mods.ctrl || mods.alt || mods.logo)
}

impl Browser {
    pub(super) fn next_page_id(&mut self) -> TabId {
        let id = TabId::from_raw(self.next_id);
        self.next_id += 1;
        id
    }

    pub(crate) fn page(&self, id: TabId) -> Option<&Tab> {
        self.peek
            .as_ref()
            .map(|peek| &peek.page)
            .filter(|page| page.id() == id)
            .or_else(|| self.tab(id))
    }

    pub(crate) fn input_page(&self) -> Option<TabId> {
        self.peek
            .as_ref()
            .map(|peek| peek.page.id())
            .or(self.active)
    }

    pub(super) fn open_peek(&mut self, source: TabId, url: String, request: u64) -> bool {
        let is_preview = self
            .peek
            .as_ref()
            .is_some_and(|peek| peek.page.id() == source);
        if (!is_preview && !self.visible_tabs().contains(&Some(source)))
            || !matches!(
                url.split_once(':').map(|(scheme, _)| scheme),
                Some("http" | "https" | "file")
            )
            || self.engine.is_none()
        {
            self.send(source, ViewCommand::DiscardPrepared(request));
            return false;
        }
        self.reset_input();
        let previous = self.peek.take();
        let [width, height] = self.window.viewport;
        let origin = self
            .peek_origin
            .take()
            .filter(|(owner, _)| *owner == source)
            .map_or([width / 2.0, height / 2.0], |(_, point)| point);
        let id = self.next_page_id();
        let mut page = Tab::new(id, url);
        page.engine.pending_load = None;
        page.loading = true;
        if let Some(engine) = &self.engine {
            if let Some(old) = &previous
                && old.page.engine.focused
            {
                engine.send(Command::View(old.page.id(), ViewCommand::Focus(false)));
            }
            engine.send(Command::Open(id, page.surface.clone()));
        }
        self.peek = Some(PeekState { page, origin });
        self.sync_engine();
        // A replacement originating inside Peek still needs the previous view's descriptor.
        self.send(id, ViewCommand::LoadPrepared { source, request });
        if let Some(old) = previous {
            self.destroy_peek(old);
        }
        true
    }

    pub(crate) fn dismiss_peek(&mut self) -> bool {
        if self.peek.is_none() {
            return false;
        }
        self.reset_input();
        if let Some(peek) = self.peek.take() {
            self.destroy_peek(peek);
        }
        true
    }

    fn destroy_peek(&self, peek: PeekState) {
        if let Some(engine) = &self.engine {
            engine.send(Command::Close(peek.page.id()));
        }
    }

    pub(crate) fn promote_peek(&mut self, destination: Destination) -> Option<TabId> {
        self.reset_input();
        let PeekState { page, .. } = self.peek.take()?;
        let id = page.id();
        if page.committed_url != "about:blank" && page.error.is_none() {
            self.promoted_visit = Some((id, page.committed_url.clone(), page.title.clone()));
        }
        self.tabs.push(page);
        match destination {
            Destination::Tab => self.exit_split(Some(id)),
            Destination::Split => {
                // If the split picker was on the left, keep the existing right page on the left.
                if self.split.is_some_and(|split| split.panes[0].is_none()) {
                    self.swap_split();
                }
                self.place_split(id, 1);
            }
        }
        Some(id)
    }

    pub(super) fn note_peek_origin(&mut self, id: TabId, point: [f32; 2]) {
        let [width, _] = self.window.viewport;
        let [x, y] = if self.peek.as_ref().is_some_and(|peek| peek.page.id() == id) {
            let layout = geometry(self.window.viewport);
            [layout.x, layout.y]
        } else if let Some(split) = self.split.filter(|split| split.panes[1] == Some(id)) {
            [split::widths(width, split.ratio)[0] + split::DIVIDER, 0.0]
        } else {
            [0.0; 2]
        };
        self.peek_origin = Some((id, [x + point[0], y + point[1]]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Engine, FakeEngine, TabUpdate, Update};

    const TARGET: &str = "https://preview.example/";
    const SOURCE: &str = "https://source.example/";

    fn browser() -> (Browser, FakeEngine, TabId) {
        let mut browser = Browser::default();
        let (engine, fake) = Engine::fake();
        browser.connect(engine);
        let source = browser.open_tab(SOURCE).unwrap();
        browser.note_dark(false);
        browser.set_viewport([1000.0, 600.0]);
        browser.poll();
        fake.commands.try_iter().for_each(drop);
        (browser, fake, source)
    }

    fn open(browser: &mut Browser, fake: &FakeEngine, source: TabId, request: u64) -> TabId {
        fake.updates
            .send(Update::Peek(source, TARGET.into(), request))
            .unwrap();
        assert!(browser.poll().changed);
        browser.peek.as_ref().unwrap().page.id()
    }

    #[test]
    fn opens_a_temporary_page_without_recording_visits() {
        let (mut browser, fake, source) = browser();
        browser.note_peek_origin(source, [24.0, 38.0]);
        let preview = open(&mut browser, &fake, source, 1);
        assert_eq!(browser.tabs().len(), 1);
        assert_eq!(browser.active, Some(source));
        assert_eq!(browser.input_page(), Some(preview));
        assert_eq!(browser.peek.as_ref().unwrap().origin, [24.0, 38.0]);
        let commands: Vec<_> = fake.commands.try_iter().collect();
        assert!(matches!(commands.first(), Some(Command::Open(id, _)) if *id == preview));
        assert!(
            matches!(commands.last(), Some(Command::View(id, ViewCommand::LoadPrepared { source: from, request: 1 })) if *id == preview && *from == source)
        );
        fake.updates
            .send(Update::Tab(
                preview,
                TabUpdate::NavigationCommitted(TARGET.into()),
            ))
            .unwrap();
        fake.updates
            .send(Update::Tab(preview, TabUpdate::Title("Preview".into())))
            .unwrap();
        let polled = browser.poll();
        assert!(polled.committed.is_empty());
        assert!(polled.titles.is_empty());
        assert_eq!(browser.page(preview).unwrap().title, "Preview");
    }

    #[test]
    fn replacing_a_preview_loads_from_its_view_before_closing_it() {
        let (mut browser, fake, source) = browser();
        let first = open(&mut browser, &fake, source, 1);
        fake.commands.try_iter().for_each(drop);
        let second = open(&mut browser, &fake, first, 2);
        assert_ne!(first, second);
        let commands: Vec<_> = fake.commands.try_iter().collect();
        assert_eq!(
            commands[commands.len() - 2..],
            [
                Command::View(
                    second,
                    ViewCommand::LoadPrepared {
                        source: first,
                        request: 2
                    }
                ),
                Command::Close(first)
            ]
        );
        fake.updates
            .send(Update::Tab(first, TabUpdate::Title("Stale".into())))
            .unwrap();
        browser.poll();
        assert_ne!(browser.page(second).unwrap().title, "Stale");
        assert_eq!(browser.tabs().len(), 1);
    }

    #[test]
    fn dismisses_once_and_leaves_the_source_intact() {
        let (mut browser, fake, source) = browser();
        let preview = open(&mut browser, &fake, source, 1);
        fake.commands.try_iter().for_each(drop);
        assert!(browser.dismiss_peek());
        assert!(!browser.dismiss_peek());
        assert_eq!(
            fake.commands.try_iter().collect::<Vec<_>>(),
            [Command::Close(preview)]
        );
        assert_eq!(browser.input_page(), Some(source));
        assert_eq!(browser.tabs()[0].url, SOURCE);
        assert_eq!(browser.recent_tabs(), [source]);
    }

    #[test]
    fn promotes_the_same_live_view_into_a_tab_and_records_its_visit() {
        let (mut browser, fake, source) = browser();
        let preview = open(&mut browser, &fake, source, 1);
        fake.updates
            .send(Update::Tab(
                preview,
                TabUpdate::NavigationCommitted(TARGET.into()),
            ))
            .unwrap();
        browser.poll();
        let surface = browser.page(preview).unwrap().surface.clone();
        fake.commands.try_iter().for_each(drop);
        assert_eq!(browser.promote_peek(Destination::Tab), Some(preview));
        assert!(browser.peek.is_none());
        assert_eq!(browser.active, Some(preview));
        assert_eq!(browser.tabs().len(), 2);
        assert_eq!(browser.tab(preview).unwrap().surface, surface);
        assert_eq!(
            browser.poll().committed,
            [(preview, TARGET.into(), String::new())]
        );
        assert!(fake.commands.try_iter().all(|command| matches!(
            command,
            Command::View(
                _,
                ViewCommand::Resize { .. } | ViewCommand::Visible(_) | ViewCommand::Focus(_)
            )
        )));
    }

    #[test]
    fn promotes_to_the_right_pane_and_leaves_replaced_tabs_open() {
        let (mut browser, fake, source) = browser();
        let first = open(&mut browser, &fake, source, 1);
        assert_eq!(browser.promote_peek(Destination::Split), Some(first));
        assert_eq!(browser.visible_tabs(), [Some(source), Some(first)]);
        let second = open(&mut browser, &fake, first, 2);
        assert_eq!(browser.promote_peek(Destination::Split), Some(second));
        assert_eq!(browser.visible_tabs(), [Some(source), Some(second)]);
        assert_eq!(browser.active, Some(second));
        assert!(browser.tab(first).is_some());
        assert!(browser.peek.is_none());
    }

    #[test]
    fn discards_unsupported_requests_and_reports_load_errors_inline() {
        let (mut browser, fake, source) = browser();
        fake.updates
            .send(Update::Peek(source, "javascript:void(0)".into(), 1))
            .unwrap();
        assert!(!browser.poll().changed);
        assert_eq!(
            fake.commands.try_iter().collect::<Vec<_>>(),
            [Command::View(source, ViewCommand::DiscardPrepared(1))]
        );
        let preview = open(&mut browser, &fake, source, 2);
        fake.updates
            .send(Update::Tab(preview, TabUpdate::LoadError("Offline".into())))
            .unwrap();
        assert!(browser.poll().changed);
        assert_eq!(
            browser.page(preview).unwrap().error.as_deref(),
            Some("Offline")
        );
        assert!(browser.active_tab().unwrap().error.is_none());
    }
}
