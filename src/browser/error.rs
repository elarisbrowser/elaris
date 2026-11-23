// SPDX-License-Identifier: GPL-3.0-only

//! The reasons a browser operation can fail.

use std::fmt;

use elaris_core::downloads as download_types;

use crate::tab::TabId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Error {
    /// The address cannot be loaded, for the reason given.
    Address(&'static str),
    /// No open tab has this id.
    NoSuchTab(TabId),
    /// Find in Page is not available on Elaris-owned internal pages.
    InternalPage(TabId),
    /// The requested position is outside the current tab order.
    #[cfg(test)]
    InvalidTabIndex(usize),
    /// The requested position is outside the destination section.
    InvalidSectionIndex(usize),
    /// The requested download operation could not be queued.
    Download(download_types::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(reason) => f.write_str(reason),
            Self::NoSuchTab(_) => f.write_str("The tab has closed"),
            Self::InternalPage(_) => f.write_str("Find in Page is unavailable on internal pages"),
            #[cfg(test)]
            Self::InvalidTabIndex(_) => f.write_str("The tab position is out of range"),
            Self::InvalidSectionIndex(_) => f.write_str("The section position is out of range"),
            Self::Download(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for Error {}
