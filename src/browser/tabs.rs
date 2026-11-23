// SPDX-License-Identifier: GPL-3.0-only

//! The ordered tab collection: finding tabs, their sections, and their order.

use super::Browser;
use super::Error;
use crate::tab::{Tab, TabId, TabSection};
use elaris_core::internal::{InternalPage, InternalRoute};

pub(super) fn section_rank(section: TabSection) -> usize {
    match section {
        TabSection::Favourite => 0,
        TabSection::Regular => 1,
    }
}

impl Browser {
    pub(crate) fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub(crate) fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|tab| tab.id() == id)
    }

    pub(crate) fn restore_favicon(&mut self, id: TabId, width: u32, height: u32, rgba: Vec<u8>) {
        let Some(tab) = self.tab_mut(id) else { return };
        tab.update(crate::engine::TabUpdate::Favicon(Some(
            elaris::ladybird::Favicon {
                width,
                height,
                rgba,
            },
        )));
    }

    /// The tab `offset` places from `id` in the list, wrapping around its ends.
    pub(crate) fn tab_beside(&self, id: TabId, offset: isize) -> Option<TabId> {
        let index = self.tabs.iter().position(|tab| tab.id() == id)?;
        let len = self.tabs.len() as isize;
        let beside = (index as isize + offset).rem_euclid(len) as usize;
        Some(self.tabs[beside].id())
    }

    /// Moves an open tab to its zero-based position without touching its page or active state.
    #[cfg(test)]
    pub(crate) fn move_tab(&mut self, id: TabId, target_index: usize) -> Result<(), Error> {
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id() == id) else {
            return Err(Error::NoSuchTab(id));
        };
        let section = self.tabs[source_index].section();
        let section_index = self.tabs[..source_index]
            .iter()
            .filter(|tab| tab.section() == section)
            .count();
        let section_len = self
            .tabs
            .iter()
            .filter(|tab| tab.section() == section)
            .count();
        if target_index >= section_len {
            return Err(Error::InvalidTabIndex(target_index));
        }
        if target_index == section_index {
            return Ok(());
        }
        self.move_tab_to(id, section, target_index)
    }

    /// Moves a live tab into `section` at its zero-based position without changing its page or
    /// active state. The single tab vector stays grouped in keyboard order: favourites,
    /// then regular tabs.
    pub(crate) fn move_tab_to(
        &mut self,
        id: TabId,
        section: TabSection,
        target_index: usize,
    ) -> Result<(), Error> {
        let Some(source_index) = self.tabs.iter().position(|tab| tab.id() == id) else {
            return Err(Error::NoSuchTab(id));
        };
        let source_section = self.tabs[source_index].section();
        let source_section_index = self.tabs[..source_index]
            .iter()
            .filter(|tab| tab.section() == source_section)
            .count();
        let target_len = self
            .tabs
            .iter()
            .filter(|tab| tab.section() == section)
            .count()
            - usize::from(source_section == section);
        if target_index > target_len {
            return Err(Error::InvalidSectionIndex(target_index));
        }
        if source_section == section && source_section_index == target_index {
            return Ok(());
        }

        let mut tab = self.tabs.remove(source_index);
        tab.section = section;
        let section_start = self
            .tabs
            .iter()
            .position(|candidate| section_rank(candidate.section()) >= section_rank(section))
            .unwrap_or(self.tabs.len());
        self.tabs.insert(section_start + target_index, tab);
        Ok(())
    }

    pub(crate) fn section_tabs(&self, section: TabSection) -> impl Iterator<Item = &Tab> {
        self.tabs.iter().filter(move |tab| tab.section() == section)
    }

    pub(crate) fn internal_tab(&self, page: InternalPage) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|tab| tab.internal == Some(InternalRoute::Page(page)))
            .map(Tab::id)
    }

    pub(crate) fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|tab| tab.id() == id)
    }
}
