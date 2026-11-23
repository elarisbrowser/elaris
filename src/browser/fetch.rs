// SPDX-License-Identifier: GPL-3.0-only

//! Fetching a page's title and icon without a tab: a hidden, muted view loads the page, reports
//! what it found, and closes once it has both, or when its time is up.

use std::time::{Duration, Instant};

use super::Browser;
use crate::engine::{Command, Surface, TabUpdate, ViewCommand};
use crate::tab::TabId;
use elaris::ladybird::MediaAction;
use elaris_core::bookmarks::Favicon;

/// How many pages load at once, so fetching never crowds out the user's tabs.
const MAX_FETCHES: usize = 2;
/// How long a page has to report its title and icon.
const FETCH_TIMEOUT: Duration = Duration::from_secs(15);
/// How long after the page finished loading its icon may still arrive.
const ICON_GRACE: Duration = Duration::from_secs(2);
/// The size the hidden view lays out at, as a small window would.
const FETCH_VIEWPORT: (i32, i32) = (800, 600);

pub(super) struct Fetch {
    id: TabId,
    url: String,
    started: Instant,
    title: Option<String>,
    favicon: Option<Favicon>,
    /// The page started loading, and when it finished.
    loading: bool,
    loaded: Option<Instant>,
}

/// What a fetched page reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fetched {
    pub(crate) url: String,
    pub(crate) title: Option<String>,
    pub(crate) favicon: Option<Favicon>,
}

impl Fetch {
    /// Closes the hidden view; nothing more is reported for it.
    pub(super) fn close(&self, engine: Option<&crate::engine::Engine>) {
        if let Some(engine) = engine {
            engine.discard_tab_updates(self.id);
            engine.send(Command::Close(self.id));
        }
        self.id.release();
    }
}

impl Browser {
    /// Starts loading `url` in a hidden view, unless it is loading already, too many are, or there
    /// is no engine. Private windows fetch nothing, as their pages leave no trace.
    pub(crate) fn fetch_page(&mut self, url: &str) -> bool {
        if self.private
            || self.fetches.len() >= MAX_FETCHES
            || self.fetches.iter().any(|fetch| fetch.url == url)
        {
            return false;
        }
        let Some(engine) = &self.engine else {
            return false;
        };
        let id = TabId::next();
        let send = |command| engine.send(Command::View(id, command));
        engine.send(Command::Open(id, Surface::default(), false));
        send(ViewCommand::MediaAction(MediaAction::ToggleMute));
        send(ViewCommand::Visible(false));
        send(ViewCommand::Resize {
            width: FETCH_VIEWPORT.0,
            height: FETCH_VIEWPORT.1,
            scale: 1.0,
        });
        send(ViewCommand::Load(url.to_owned()));
        self.fetches.push(Fetch {
            id,
            url: url.to_owned(),
            started: Instant::now(),
            title: None,
            favicon: None,
            loading: false,
            loaded: None,
        });
        true
    }

    pub(super) fn fetch_ids(&self) -> impl Iterator<Item = TabId> + '_ {
        self.fetches.iter().map(|fetch| fetch.id)
    }

    /// Takes in what a hidden view reported; returns false for an update that isn't a fetch's.
    pub(super) fn fetch_update(&mut self, id: TabId, update: &TabUpdate) -> bool {
        let Some(fetch) = self.fetches.iter_mut().find(|fetch| fetch.id == id) else {
            return false;
        };
        match update {
            TabUpdate::Title(title) => fetch.title = Some(title.clone()),
            TabUpdate::Favicon(Some(favicon)) => {
                fetch.favicon = Favicon::new(favicon.width, favicon.height, &favicon.rgba);
            }
            TabUpdate::Loading(true) => fetch.loading = true,
            TabUpdate::Loading(false) if fetch.loading => {
                fetch.loading = false;
                fetch.loaded = Some(Instant::now());
            }
            _ => {}
        }
        true
    }

    /// Closes the fetches that have what they came for, finished loading a little while ago, or
    /// ran out of time, and returns what each found.
    pub(super) fn finish_fetches(&mut self) -> Vec<Fetched> {
        let now = Instant::now();
        let (done, pending): (Vec<_>, Vec<_>) = std::mem::take(&mut self.fetches)
            .into_iter()
            .partition(|fetch| {
                (fetch.title.is_some() && fetch.favicon.is_some())
                    || fetch
                        .loaded
                        .is_some_and(|loaded| now.duration_since(loaded) >= ICON_GRACE)
                    || now.duration_since(fetch.started) >= FETCH_TIMEOUT
            });
        self.fetches = pending;
        done.into_iter()
            .map(|fetch| {
                fetch.close(self.engine.as_ref());
                Fetched {
                    url: fetch.url,
                    title: fetch.title.filter(|title| !title.trim().is_empty()),
                    favicon: fetch.favicon,
                }
            })
            .collect()
    }

    /// Whether a fetch is still loading, so the window keeps polling until it is done.
    pub(crate) fn fetching(&self) -> bool {
        !self.fetches.is_empty()
    }
}
