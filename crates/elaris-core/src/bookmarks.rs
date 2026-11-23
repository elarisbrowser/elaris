// SPDX-License-Identifier: GPL-3.0-only

//! Bookmarks: saved links in a tree of folders, kept as JSON beside the history.
//!
//! Two folders always exist and can't be removed or moved: the Bookmarks Bar, whose items the
//! horizontal layout shows in its bar, and Other Bookmarks. Every node has an ID that is never
//! reused, so a stale ID can only miss.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file's format; a newer one is not read, so it is never overwritten with less.
const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BookmarkId(u64);

impl BookmarkId {
    /// The Bookmarks Bar folder.
    pub const BAR: Self = Self(1);
    /// The Other Bookmarks folder.
    pub const OTHER: Self = Self(2);

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// One of the two folders that always exist.
    pub fn is_root(self) -> bool {
        self == Self::BAR || self == Self::OTHER
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: BookmarkId,
    pub title: String,
    /// When it was added, in seconds since the Unix epoch.
    #[serde(default)]
    pub added: u64,
    /// The page's icon, kept so lists can show it without the page open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favicon: Option<Favicon>,
    #[serde(flatten)]
    pub kind: Kind,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Link { url: String },
    Folder { children: Vec<Bookmark> },
}

impl Bookmark {
    pub fn url(&self) -> Option<&str> {
        match &self.kind {
            Kind::Link { url } => Some(url),
            Kind::Folder { .. } => None,
        }
    }

    pub fn children(&self) -> &[Bookmark] {
        match &self.kind {
            Kind::Folder { children } => children,
            Kind::Link { .. } => &[],
        }
    }

    pub fn is_folder(&self) -> bool {
        matches!(self.kind, Kind::Folder { .. })
    }

    /// What the bar and lists call it: its title, or its site until it has one.
    pub fn label(&self) -> &str {
        match (self.title.trim(), self.url()) {
            ("", Some(url)) => crate::address::compact_address(url),
            (title, _) => title,
        }
    }

    /// A link still without a title or icon, which loading its page would give it.
    pub fn missing_details(&self) -> bool {
        self.url().is_some() && (self.title.trim().is_empty() || self.favicon.is_none())
    }

    fn children_mut(&mut self) -> Option<&mut Vec<Bookmark>> {
        match &mut self.kind {
            Kind::Folder { children } => Some(children),
            Kind::Link { .. } => None,
        }
    }
}

/// A page icon, at most [`Favicon::MAX`] pixels on a side, as RGBA rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Favicon {
    pub width: u32,
    pub height: u32,
    #[serde(with = "hex")]
    pub rgba: Vec<u8>,
}

impl Favicon {
    /// Larger icons are scaled down to this, which lists never draw bigger.
    pub const MAX: u32 = 32;

    /// An icon from a page's RGBA pixels, scaled down if larger than [`Favicon::MAX`]; none if
    /// the pixels don't fill the size.
    pub fn new(width: u32, height: u32, rgba: &[u8]) -> Option<Self> {
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        if width == 0 || height == 0 || rgba.len() != expected {
            return None;
        }
        let scale = (width.max(height) as f32 / Self::MAX as f32).max(1.0);
        let (new_width, new_height) = (
            ((width as f32 / scale).round() as u32).max(1),
            ((height as f32 / scale).round() as u32).max(1),
        );
        let mut scaled = Vec::with_capacity(new_width as usize * new_height as usize * 4);
        for y in 0..new_height {
            let source_y = ((y as f32 * scale) as u32).min(height - 1);
            for x in 0..new_width {
                let source_x = ((x as f32 * scale) as u32).min(width - 1);
                let at = (source_y as usize * width as usize + source_x as usize) * 4;
                scaled.extend_from_slice(&rgba[at..at + 4]);
            }
        }
        Some(Self {
            width: new_width,
            height: new_height,
            rgba: scaled,
        })
    }
}

/// Bytes as lowercase hex, which keeps icons compact in the JSON.
mod hex {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        let text: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        serializer.serialize_str(&text)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        if text.len() % 2 != 0 {
            return Err(D::Error::custom("hex has an odd length"));
        }
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).map_err(D::Error::custom))
            .collect()
    }
}

/// Why a change could not be made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NotFound,
    NotAFolder,
    /// The Bookmarks Bar and Other Bookmarks stay where they are.
    Root,
    /// A folder can't go inside itself.
    IntoItself,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NotFound => "That bookmark no longer exists",
            Self::NotAFolder => "Bookmarks can only go in folders",
            Self::Root => "The Bookmarks Bar and Other Bookmarks can't be changed",
            Self::IntoItself => "A folder can't go inside itself",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmarks {
    version: u32,
    next_id: u64,
    bar: Bookmark,
    other: Bookmark,
}

impl Default for Bookmarks {
    fn default() -> Self {
        let folder = |id, title: &str| Bookmark {
            id,
            title: title.to_owned(),
            added: 0,
            favicon: None,
            kind: Kind::Folder {
                children: Vec::new(),
            },
        };
        Self {
            version: VERSION,
            next_id: 3,
            bar: folder(BookmarkId::BAR, "Bookmarks Bar"),
            other: folder(BookmarkId::OTHER, "Other Bookmarks"),
        }
    }
}

impl Bookmarks {
    /// The Bookmarks Bar and Other Bookmarks, in that order.
    pub fn roots(&self) -> [&Bookmark; 2] {
        [&self.bar, &self.other]
    }

    pub fn bar(&self) -> &[Bookmark] {
        self.bar.children()
    }

    pub fn get(&self, id: BookmarkId) -> Option<&Bookmark> {
        self.roots().into_iter().find_map(|root| find(root, id))
    }

    /// The folder holding `id`, and its position there.
    pub fn parent(&self, id: BookmarkId) -> Option<(BookmarkId, usize)> {
        fn search(folder: &Bookmark, id: BookmarkId) -> Option<(BookmarkId, usize)> {
            let children = folder.children();
            if let Some(index) = children.iter().position(|child| child.id == id) {
                return Some((folder.id, index));
            }
            children.iter().find_map(|child| search(child, id))
        }
        self.roots().into_iter().find_map(|root| search(root, id))
    }

    /// The folders from a root down to the one holding `id`, for a breadcrumb.
    pub fn path(&self, id: BookmarkId) -> Vec<&Bookmark> {
        let mut path = Vec::new();
        let mut current = id;
        while let Some((parent, _)) = self.parent(current) {
            path.extend(self.get(parent));
            current = parent;
        }
        path.reverse();
        path
    }

    /// The first bookmark of this address, if any.
    pub fn find_url(&self, url: &str) -> Option<&Bookmark> {
        self.links().find(|link| link.url() == Some(url))
    }

    /// Every link, depth first, as they are listed.
    pub fn links(&self) -> impl Iterator<Item = &Bookmark> {
        let mut stack: Vec<&Bookmark> = vec![&self.other, &self.bar];
        std::iter::from_fn(move || {
            while let Some(node) = stack.pop() {
                if node.is_folder() {
                    stack.extend(node.children().iter().rev());
                } else {
                    return Some(node);
                }
            }
            None
        })
    }

    /// Every folder, each with how deep it sits, for a folder picker.
    pub fn folders(&self) -> Vec<(usize, &Bookmark)> {
        fn walk<'a>(folder: &'a Bookmark, depth: usize, out: &mut Vec<(usize, &'a Bookmark)>) {
            out.push((depth, folder));
            for child in folder.children().iter().filter(|child| child.is_folder()) {
                walk(child, depth + 1, out);
            }
        }
        let mut folders = Vec::new();
        for root in self.roots() {
            walk(root, 0, &mut folders);
        }
        folders
    }

    /// Links whose title or address contains every word of `query`, ignoring case.
    pub fn search(&self, query: &str) -> Vec<&Bookmark> {
        let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
        if words.is_empty() {
            return Vec::new();
        }
        self.links()
            .filter(|link| {
                let haystack = format!(
                    "{} {}",
                    link.title.to_lowercase(),
                    link.url().unwrap_or_default().to_lowercase()
                );
                words.iter().all(|word| haystack.contains(word))
            })
            .collect()
    }

    /// Adds a link at the end of `folder`.
    pub fn add_link(
        &mut self,
        folder: BookmarkId,
        title: &str,
        url: &str,
        added: u64,
    ) -> Result<BookmarkId, Error> {
        self.insert(
            folder,
            title,
            Kind::Link {
                url: url.to_owned(),
            },
            added,
        )
    }

    /// Adds an empty folder at the end of `folder`.
    pub fn add_folder(
        &mut self,
        folder: BookmarkId,
        title: &str,
        added: u64,
    ) -> Result<BookmarkId, Error> {
        self.insert(
            folder,
            title,
            Kind::Folder {
                children: Vec::new(),
            },
            added,
        )
    }

    fn insert(
        &mut self,
        folder: BookmarkId,
        title: &str,
        kind: Kind,
        added: u64,
    ) -> Result<BookmarkId, Error> {
        let id = BookmarkId(self.next_id);
        let children = self
            .get_mut(folder)
            .ok_or(Error::NotFound)?
            .children_mut()
            .ok_or(Error::NotAFolder)?;
        children.push(Bookmark {
            id,
            title: title.trim().to_owned(),
            added,
            favicon: None,
            kind,
        });
        self.next_id += 1;
        Ok(id)
    }

    /// Fills in what bookmarks of `url` lack: an empty title from `title`, a missing icon from
    /// `favicon`. Reports whether any changed; a title the user gave is never replaced.
    pub fn fill_details(
        &mut self,
        url: &str,
        title: Option<&str>,
        favicon: Option<&Favicon>,
    ) -> bool {
        fn walk(
            node: &mut Bookmark,
            url: &str,
            title: Option<&str>,
            favicon: Option<&Favicon>,
        ) -> bool {
            let mut changed = false;
            if node.url() == Some(url) {
                if let Some(title) = title.map(str::trim).filter(|title| !title.is_empty())
                    && node.title.trim().is_empty()
                {
                    node.title = title.to_owned();
                    changed = true;
                }
                if node.favicon.is_none()
                    && let Some(favicon) = favicon
                {
                    node.favicon = Some(favicon.clone());
                    changed = true;
                }
            }
            if let Some(children) = node.children_mut() {
                for child in children {
                    changed |= walk(child, url, title, favicon);
                }
            }
            changed
        }
        walk(&mut self.bar, url, title, favicon) | walk(&mut self.other, url, title, favicon)
    }

    /// Renames a bookmark, and changes a link's address when `url` is given.
    pub fn edit(&mut self, id: BookmarkId, title: &str, url: Option<&str>) -> Result<(), Error> {
        if id.is_root() {
            return Err(Error::Root);
        }
        let bookmark = self.get_mut(id).ok_or(Error::NotFound)?;
        bookmark.title = title.trim().to_owned();
        if let (Some(new_url), Kind::Link { url }) = (url, &mut bookmark.kind) {
            *url = new_url.to_owned();
        }
        Ok(())
    }

    /// Removes a bookmark, and everything in it if it is a folder.
    pub fn remove(&mut self, id: BookmarkId) -> Result<Bookmark, Error> {
        if id.is_root() {
            return Err(Error::Root);
        }
        let (parent, index) = self.parent(id).ok_or(Error::NotFound)?;
        let children = self
            .get_mut(parent)
            .and_then(Bookmark::children_mut)
            .ok_or(Error::NotFound)?;
        Ok(children.remove(index))
    }

    /// Moves a bookmark into `folder` at `index`, which counts the folder's items without it.
    pub fn move_to(
        &mut self,
        id: BookmarkId,
        folder: BookmarkId,
        index: usize,
    ) -> Result<(), Error> {
        if id.is_root() {
            return Err(Error::Root);
        }
        let target = self.get(folder).ok_or(Error::NotFound)?;
        if !target.is_folder() {
            return Err(Error::NotAFolder);
        }
        if folder == id || self.path(folder).iter().any(|ancestor| ancestor.id == id) {
            return Err(Error::IntoItself);
        }
        let node = self.remove(id)?;
        let children = self
            .get_mut(folder)
            .and_then(Bookmark::children_mut)
            .ok_or(Error::NotFound)?;
        let index = index.min(children.len());
        children.insert(index, node);
        Ok(())
    }

    fn get_mut(&mut self, id: BookmarkId) -> Option<&mut Bookmark> {
        fn find_mut(node: &mut Bookmark, id: BookmarkId) -> Option<&mut Bookmark> {
            if node.id == id {
                return Some(node);
            }
            node.children_mut()?
                .iter_mut()
                .find_map(|child| find_mut(child, id))
        }
        find_mut(&mut self.bar, id).or_else(|| find_mut(&mut self.other, id))
    }

    /// The JSON the storage worker writes.
    pub fn encode(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Reads the bookmarks file; none yet means none saved.
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        };
        Self::decode(&text)
    }

    pub fn decode(text: &str) -> io::Result<Self> {
        let bookmarks: Self = serde_json::from_str(text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if bookmarks.version > VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bookmarks were saved by a newer Elaris",
            ));
        }
        Ok(bookmarks)
    }
}

fn find(node: &Bookmark, id: BookmarkId) -> Option<&Bookmark> {
    if node.id == id {
        return Some(node);
    }
    node.children().iter().find_map(|child| find(child, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> (Bookmarks, BookmarkId, BookmarkId, BookmarkId) {
        let mut bookmarks = Bookmarks::default();
        let news = bookmarks.add_folder(BookmarkId::BAR, "News", 1).unwrap();
        let bbc = bookmarks
            .add_link(news, "BBC", "https://bbc.co.uk/", 2)
            .unwrap();
        let docs = bookmarks
            .add_link(BookmarkId::OTHER, " Docs ", "https://docs.rs/", 3)
            .unwrap();
        (bookmarks, news, bbc, docs)
    }

    #[test]
    fn links_and_folders_are_added_where_asked_with_ids_never_reused() {
        let (mut bookmarks, news, bbc, docs) = sample();
        assert_eq!(bookmarks.bar()[0].id, news);
        assert_eq!(
            bookmarks.get(bbc).unwrap().url(),
            Some("https://bbc.co.uk/")
        );
        assert_eq!(bookmarks.get(docs).unwrap().title, "Docs");
        assert_eq!(bookmarks.parent(bbc), Some((news, 0)));
        assert_eq!(
            bookmarks.add_link(bbc, "x", "https://x/", 0),
            Err(Error::NotAFolder)
        );
        bookmarks.remove(docs).unwrap();
        let next = bookmarks
            .add_link(BookmarkId::OTHER, "", "https://e.com/", 4)
            .unwrap();
        assert!(next.get() > docs.get());
        assert_eq!(bookmarks.get(next).unwrap().label(), "e.com");
    }

    #[test]
    fn links_are_listed_bar_first_and_found_by_address_and_words() {
        let (bookmarks, _, bbc, docs) = sample();
        let links: Vec<_> = bookmarks.links().map(|link| link.id).collect();
        assert_eq!(links, [bbc, docs]);
        assert_eq!(bookmarks.find_url("https://docs.rs/").unwrap().id, docs);
        assert_eq!(bookmarks.search("bbc CO").len(), 1);
        assert!(bookmarks.search("bbc docs").is_empty());
        assert!(bookmarks.search("  ").is_empty());
    }

    #[test]
    fn moves_keep_roots_in_place_and_folders_out_of_themselves() {
        let (mut bookmarks, news, bbc, docs) = sample();
        bookmarks.move_to(docs, news, 0).unwrap();
        assert_eq!(bookmarks.parent(docs), Some((news, 0)));
        assert_eq!(bookmarks.parent(bbc), Some((news, 1)));
        let inner = bookmarks.add_folder(news, "Inner", 5).unwrap();
        assert_eq!(bookmarks.move_to(news, inner, 0), Err(Error::IntoItself));
        assert_eq!(bookmarks.move_to(news, news, 0), Err(Error::IntoItself));
        assert_eq!(
            bookmarks.move_to(BookmarkId::BAR, BookmarkId::OTHER, 0),
            Err(Error::Root)
        );
        assert_eq!(bookmarks.remove(BookmarkId::OTHER), Err(Error::Root));
        assert_eq!(
            bookmarks.path(bbc).iter().map(|f| f.id).collect::<Vec<_>>(),
            [BookmarkId::BAR, news]
        );
        let folders: Vec<_> = bookmarks
            .folders()
            .iter()
            .map(|(depth, folder)| (*depth, folder.id))
            .collect();
        assert_eq!(
            folders,
            [
                (0, BookmarkId::BAR),
                (1, news),
                (2, inner),
                (0, BookmarkId::OTHER)
            ]
        );
    }

    #[test]
    fn edits_rename_and_readdress_but_never_a_root() {
        let (mut bookmarks, news, bbc, _) = sample();
        bookmarks
            .edit(bbc, "BBC News", Some("https://bbc.com/"))
            .unwrap();
        assert_eq!(bookmarks.get(bbc).unwrap().url(), Some("https://bbc.com/"));
        bookmarks.edit(news, "Daily", Some("ignored")).unwrap();
        assert_eq!(bookmarks.get(news).unwrap().title, "Daily");
        assert_eq!(bookmarks.edit(BookmarkId::BAR, "x", None), Err(Error::Root));
    }

    #[test]
    fn page_details_fill_only_what_is_missing() {
        let mut bookmarks = Bookmarks::default();
        let blank = bookmarks
            .add_link(BookmarkId::BAR, "", "https://www.google.com/search?q=hi", 0)
            .unwrap();
        let named = bookmarks
            .add_link(
                BookmarkId::OTHER,
                "Mine",
                "https://www.google.com/search?q=hi",
                0,
            )
            .unwrap();
        assert_eq!(bookmarks.get(blank).unwrap().label(), "google.com");
        assert!(bookmarks.get(blank).unwrap().missing_details());
        let icon = Favicon::new(1, 1, &[1, 2, 3, 4]).unwrap();
        assert!(bookmarks.fill_details(
            "https://www.google.com/search?q=hi",
            Some("hi - Google Search"),
            Some(&icon)
        ));
        assert_eq!(bookmarks.get(blank).unwrap().title, "hi - Google Search");
        assert_eq!(bookmarks.get(named).unwrap().title, "Mine");
        assert_eq!(bookmarks.get(named).unwrap().favicon, Some(icon.clone()));
        assert!(!bookmarks.get(blank).unwrap().missing_details());
        assert!(!bookmarks.fill_details(
            "https://www.google.com/search?q=hi",
            Some("x"),
            Some(&icon)
        ));
    }

    #[test]
    fn large_icons_are_scaled_down_and_short_pixels_refused() {
        let big = Favicon::new(64, 64, &[7; 64 * 64 * 4]).unwrap();
        assert_eq!(
            (big.width, big.height, big.rgba.len()),
            (32, 32, 32 * 32 * 4)
        );
        assert_eq!(Favicon::new(2, 2, &[0; 3]), None);
        assert_eq!(Favicon::new(0, 2, &[]), None);
    }

    #[test]
    fn the_file_round_trips_and_a_newer_one_is_refused() {
        let (mut bookmarks, _, bbc, _) = sample();
        let icon = Favicon::new(1, 1, &[255, 0, 16, 128]).unwrap();
        bookmarks.fill_details("https://bbc.co.uk/", None, Some(&icon));
        assert!(bookmarks.encode().contains("\"ff001080\""));
        let decoded = Bookmarks::decode(&bookmarks.encode()).unwrap();
        assert_eq!(decoded, bookmarks);
        assert_eq!(decoded.get(bbc).unwrap().favicon, Some(icon));
        let newer = bookmarks
            .encode()
            .replace("\"version\": 1", "\"version\": 9");
        assert!(Bookmarks::decode(&newer).is_err());
        let missing = std::env::temp_dir().join("elaris-no-bookmarks-here.json");
        assert_eq!(Bookmarks::load(&missing).unwrap(), Bookmarks::default());
    }
}
