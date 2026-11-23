// SPDX-License-Identifier: GPL-3.0-only

//! Searching a web tab's page with Find in Page.

use super::Browser;
use super::Error;
use crate::engine::ViewCommand;
use crate::tab::TabId;

impl Browser {
    /// Searches one web tab with Ladybird's case-insensitive, all-match search.
    pub(crate) fn find(&mut self, id: TabId, query: &str) -> Result<(), Error> {
        let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Err(Error::InternalPage(id));
        }

        if query.is_empty() {
            if tab.find.query.is_empty() && !tab.find.pending() && tab.find.result.is_none() {
                return Ok(());
            }
            tab.find.clear_query();
            let command = ViewCommand::FindEnd;
            self.send(id, command);
            return Ok(());
        }

        let Some(request_id) = tab.find.begin_query(query) else {
            return Ok(());
        };
        let query = tab.find.query.clone();
        self.send(id, ViewCommand::Find { query, request_id });
        Ok(())
    }

    /// Advances the active tab's current find query and wraps as Ladybird does.
    pub(crate) fn find_next(&mut self, id: TabId) -> Result<(), Error> {
        self.find_step(id, false)
    }

    /// Moves to the previous match in the active tab's current find query.
    pub(crate) fn find_previous(&mut self, id: TabId) -> Result<(), Error> {
        self.find_step(id, true)
    }

    /// Requests selected webpage text asynchronously for Find's initial query.
    pub(crate) fn request_selected_text(&self, id: TabId, request_id: u64) -> Result<(), Error> {
        let tab = self.tab(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Err(Error::InternalPage(id));
        }
        self.send(id, ViewCommand::SelectedText { request_id });
        Ok(())
    }

    fn find_step(&mut self, id: TabId, previous: bool) -> Result<(), Error> {
        let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Err(Error::InternalPage(id));
        }
        let Some(request_id) = tab.find.begin_navigation() else {
            return Ok(());
        };
        self.send(
            id,
            if previous {
                ViewCommand::FindPrevious { request_id }
            } else {
                ViewCommand::FindNext { request_id }
            },
        );
        Ok(())
    }

    /// Clears Ladybird's highlights and invalidates pending results, retaining this tab's query.
    pub(crate) fn clear_find(&mut self, id: TabId) -> Result<(), Error> {
        let tab = self.tab_mut(id).ok_or(Error::NoSuchTab(id))?;
        if tab.is_internal() {
            return Ok(());
        }
        tab.find.invalidate();
        self.send(id, ViewCommand::FindEnd);
        Ok(())
    }

    pub(super) fn invalidate_find(&mut self, id: TabId) {
        let Some(tab) = self.tab_mut(id).filter(|tab| !tab.is_internal()) else {
            return;
        };
        let clear_highlights = tab.find.pending() || tab.find.result.is_some();
        tab.find.invalidate();
        if clear_highlights {
            self.send(id, ViewCommand::FindEnd);
        }
    }
}
