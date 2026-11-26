// SPDX-License-Identifier: GPL-3.0-only

//! Bookmarks for every window: one list the process shares and saves, the editor the star opens,
//! and opening a bookmark. Bookmarks are saved links, apart from Favourites, which are live tabs;
//! a bookmark can be added to Favourites, and a tab bookmarked.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use rune::{TextEdit, Tx};

use super::AppState;
use crate::browser::Polled;
use crate::tab::{TabId, TabSection};
use elaris_core::bookmarks::{Bookmark, BookmarkId, Bookmarks, Error, Favicon};
use elaris_core::internal::InternalPage;

pub(crate) type SharedBookmarks = Arc<Mutex<Bookmarks>>;

/// Counts changes to the shared list, so every window redraws after one.
static REVISION: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub(crate) struct BookmarksState {
    list: SharedBookmarks,
    /// The last change this window drew.
    seen: u64,
    /// The bookmark being edited in the star's popover.
    pub(crate) editor: Option<Editor>,
    /// The bar folder whose menu is open, and the folders opened within it.
    pub(crate) open_folders: Vec<BookmarkId>,
    /// The vertical sidebar's bookmarks menu is open.
    pub(crate) sidebar_menu: bool,
    /// The bookmark whose More menu is open on the bookmarks page.
    pub(crate) row_menu: Option<BookmarkId>,
    /// A bookmark bar item's context menu and the point where it was opened.
    pub(crate) bar_menu: Option<(BookmarkId, rune::PointerPress)>,
    pub(crate) page: Page,
    /// Why the last change could not be made.
    pub(crate) error: Option<String>,
    /// Pages fetched this run for their title and icon, which are never fetched again.
    fetched: std::collections::HashSet<String>,
    /// Icons as the interface draws them, by bookmark, with the hash of the pixels they show.
    icons: std::cell::RefCell<std::collections::HashMap<BookmarkId, (u64, rune::Pixels)>>,
}

/// The star's popover: the bookmark's name, address and folder.
pub(crate) struct Editor {
    pub(crate) id: BookmarkId,
    pub(crate) title: TextEdit,
    /// Empty for a folder.
    pub(crate) url: TextEdit,
    pub(crate) folder: BookmarkId,
    pub(crate) folder_menu_open: bool,
    /// Opened from the bookmarks page, which lays it out as a dialog.
    pub(crate) on_page: bool,
}

/// The bookmarks page: the folder it shows, and its search.
pub(crate) struct Page {
    pub(crate) folder: BookmarkId,
    pub(crate) search: TextEdit,
}

impl Default for Page {
    fn default() -> Self {
        Self {
            folder: BookmarkId::BAR,
            search: TextEdit::default(),
        }
    }
}

/// The process's bookmarks, seeded once from what storage read at launch.
#[cfg(not(test))]
pub(super) fn shared(storage: Option<&crate::storage::Storage>) -> SharedBookmarks {
    static SHARED: std::sync::OnceLock<SharedBookmarks> = std::sync::OnceLock::new();
    Arc::clone(SHARED.get_or_init(|| {
        let bookmarks = storage
            .and_then(crate::storage::Storage::take_bookmarks)
            .unwrap_or_default();
        Arc::new(Mutex::new(bookmarks))
    }))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

impl BookmarksState {
    #[cfg(not(test))]
    pub(super) fn with_list(list: SharedBookmarks) -> Self {
        Self {
            list,
            seen: REVISION.load(Ordering::Relaxed),
            ..Self::default()
        }
    }
}

impl AppState {
    pub(crate) fn bookmarks(&self) -> MutexGuard<'_, Bookmarks> {
        self.bookmarks
            .list
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Whether another window changed the bookmarks since this one last drew them.
    pub(super) fn poll_bookmarks(&mut self) -> bool {
        let revision = REVISION.load(Ordering::Relaxed);
        std::mem::replace(&mut self.bookmarks.seen, revision) != revision
    }

    /// Applies a change, saves the list and tells every window; or keeps why it failed.
    fn change_bookmarks<T>(
        &mut self,
        change: impl FnOnce(&mut Bookmarks) -> Result<T, Error>,
    ) -> Option<T> {
        let mut bookmarks = self.bookmarks();
        let result = change(&mut bookmarks);
        let encoded = result.is_ok().then(|| bookmarks.encode());
        drop(bookmarks);
        match result {
            Ok(value) => {
                self.bookmarks.error = None;
                if let (Some(storage), Some(encoded)) = (&self.storage, encoded)
                    && let Err(error) = storage.save_bookmarks(encoded)
                {
                    eprintln!("Elaris: {error}");
                }
                self.bookmarks.seen = REVISION.fetch_add(1, Ordering::Relaxed) + 1;
                crate::engine::wake_all_windows();
                Some(value)
            }
            Err(error) => {
                self.bookmarks.error = Some(error.to_string());
                None
            }
        }
    }

    /// Bookmarks the tab in the Bookmarks Bar, or finds its bookmark; and with `edit`, opens the
    /// editor on it.
    pub(crate) fn bookmark_tab(&mut self, tab: TabId, edit: bool) {
        let Some((title, url)) = self
            .browser
            .tab(tab)
            .filter(|tab| !tab.is_internal() && !tab.url.is_empty() && tab.url != "about:blank")
            .map(|tab| (tab.title.trim().to_owned(), tab.url.clone()))
        else {
            return;
        };
        let favicon = self
            .browser
            .tab(tab)
            .and_then(|tab| tab.favicon_data.as_ref())
            .and_then(|icon| Favicon::new(icon.width, icon.height, &icon.rgba));
        let existing = self.bookmarks().find_url(&url).map(|bookmark| bookmark.id);
        let id = existing.or_else(|| {
            self.change_bookmarks(|bookmarks| {
                let id = bookmarks.add_link(BookmarkId::BAR, &title, &url, now())?;
                bookmarks.fill_details(&url, None, favicon.as_ref());
                Ok(id)
            })
        });
        if let Some(id) = id.filter(|_| edit) {
            self.edit_bookmark(id, false);
        }
    }

    /// Fills in bookmarks' missing titles and icons: from what fetched pages found, from open tabs
    /// showing bookmarked pages, and by fetching the pages of bookmarks on show that still lack
    /// them. Reports whether any changed.
    pub(super) fn poll_bookmark_details(&mut self, polled: &Polled, tx: &mut Tx) -> bool {
        let mut found: Vec<(String, Option<String>, Option<Favicon>)> = polled
            .fetched
            .iter()
            .map(|fetched| {
                (
                    fetched.url.clone(),
                    fetched.title.clone(),
                    fetched.favicon.clone(),
                )
            })
            .collect();
        // Tabs only gain titles and icons in polls that changed something.
        if polled.changed {
            let bookmarks = self.bookmarks();
            for tab in self.browser.tabs() {
                if tab.loading || tab.is_internal() {
                    continue;
                }
                let missing = bookmarks
                    .links()
                    .any(|link| link.url() == Some(tab.url.as_str()) && link.missing_details());
                if missing {
                    let favicon = tab
                        .favicon_data
                        .as_ref()
                        .and_then(|icon| Favicon::new(icon.width, icon.height, &icon.rgba));
                    let title = (!tab.title.trim().is_empty()).then(|| tab.title.clone());
                    if title.is_some() || favicon.is_some() {
                        found.push((tab.url.clone(), title, favicon));
                    }
                }
            }
        }
        let mut changed = false;
        if !found.is_empty() {
            let mut bookmarks = self.bookmarks();
            for (url, title, favicon) in &found {
                changed |= bookmarks.fill_details(url, title.as_deref(), favicon.as_ref());
            }
            drop(bookmarks);
            if changed {
                self.change_bookmarks(|_| Ok(()));
            }
        }
        let wanted: Vec<String> = self
            .bookmarks_on_show()
            .into_iter()
            .filter(|url| !self.bookmarks.fetched.contains(url))
            .collect();
        for url in wanted {
            if self.browser.fetch_page(&url) {
                self.bookmarks.fetched.insert(url);
            }
        }
        if self.browser.fetching() {
            tx.wake_after(std::time::Duration::from_millis(500));
        }
        changed
    }

    /// The addresses of links on show in the bar, the sidebar's menu or the bookmarks page that
    /// still lack a title or icon.
    fn bookmarks_on_show(&self) -> Vec<String> {
        let bookmarks = self.bookmarks();
        let mut shown: Vec<&Bookmark> = Vec::new();
        let menu_open = self.bookmarks.sidebar_menu || !self.bookmarks.open_folders.is_empty();
        if (self.horizontal_tabs() && !bookmarks.bar().is_empty()) || menu_open {
            shown.extend(bookmarks.bar());
        }
        if let Some(folder) = self
            .bookmarks
            .open_folders
            .last()
            .and_then(|id| bookmarks.get(*id))
        {
            shown.extend(folder.children());
        }
        let on_page = self.browser.active_tab().is_some_and(|tab| {
            tab.internal
                == Some(elaris_core::internal::InternalRoute::Page(
                    InternalPage::Bookmarks,
                ))
        });
        if on_page {
            let query = self.bookmarks.page.search.text.trim();
            if query.is_empty() {
                if let Some(folder) = bookmarks.get(self.bookmarks.page.folder) {
                    shown.extend(folder.children());
                }
            } else {
                shown.extend(bookmarks.search(query));
            }
        }
        shown
            .into_iter()
            .filter(|bookmark| bookmark.missing_details())
            .filter_map(|bookmark| bookmark.url().map(str::to_owned))
            .collect()
    }

    /// A bookmark's icon as the interface draws it, made once per icon.
    pub(crate) fn bookmark_icon(&self, bookmark: &Bookmark) -> Option<rune::Pixels> {
        use std::hash::{Hash, Hasher};
        let favicon = bookmark.favicon.as_ref()?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        favicon.rgba.hash(&mut hasher);
        let hash = hasher.finish();
        let mut icons = self.bookmarks.icons.borrow_mut();
        if let Some((cached, pixels)) = icons.get(&bookmark.id)
            && *cached == hash
        {
            return Some(pixels.clone());
        }
        let pixels = rune::Pixels::new();
        pixels.set(favicon.width, favicon.height, favicon.rgba.clone());
        icons.insert(bookmark.id, (hash, pixels.clone()));
        Some(pixels)
    }

    /// The star and Cmd+D: bookmarks the active page, or edits its bookmark.
    pub(crate) fn bookmark_active_tab(&mut self) {
        if let Some(tab) = self.browser.active_tab().map(crate::tab::Tab::id) {
            self.bookmark_tab(tab, true);
        }
    }

    /// Saves every web page in the window to a new folder in Other Bookmarks, then shows it on
    /// the bookmarks page with its name ready to change.
    pub(crate) fn bookmark_all_tabs(&mut self, tx: &mut Tx) {
        let pages: Vec<(String, String)> = self
            .browser
            .tabs()
            .iter()
            .filter(|tab| !tab.is_internal() && !tab.url.is_empty() && tab.url != "about:blank")
            .map(|tab| (tab.title.trim().to_owned(), tab.url.clone()))
            .collect();
        if pages.is_empty() {
            return;
        }
        let added = now();
        let Some(folder) = self.change_bookmarks(|bookmarks| {
            let folder = bookmarks.add_folder(BookmarkId::OTHER, "Saved Tabs", added)?;
            for (title, url) in &pages {
                bookmarks.add_link(folder, title, url, added)?;
            }
            Ok(folder)
        }) else {
            return;
        };
        open_bookmarks(self, tx);
        self.bookmarks.page.folder = BookmarkId::OTHER;
        self.edit_bookmark(folder, true);
    }

    /// Opens the editor on a bookmark or folder.
    pub(crate) fn edit_bookmark(&mut self, id: BookmarkId, on_page: bool) {
        let bookmarks = self.bookmarks();
        let (Some(bookmark), Some((folder, _))) = (bookmarks.get(id), bookmarks.parent(id)) else {
            return;
        };
        let mut title = TextEdit::default();
        title.replace_text(bookmark.title.clone());
        let mut url = TextEdit::default();
        url.replace_text(bookmark.url().unwrap_or_default().to_owned());
        let editor = Editor {
            id,
            title,
            url,
            folder,
            folder_menu_open: false,
            on_page,
        };
        drop(bookmarks);
        self.bookmarks.editor = Some(editor);
    }

    /// Saves the editor's name, address and folder, and closes it.
    pub(crate) fn save_bookmark_editor(&mut self) {
        let Some(editor) = self.bookmarks.editor.take() else {
            return;
        };
        let url = editor.url.text.trim().to_owned();
        let saved = self.change_bookmarks(|bookmarks| {
            let is_link = bookmarks
                .get(editor.id)
                .ok_or(Error::NotFound)?
                .url()
                .is_some();
            bookmarks.edit(
                editor.id,
                &editor.title.text,
                (is_link && !url.is_empty()).then_some(url.as_str()),
            )?;
            if bookmarks.parent(editor.id).map(|(folder, _)| folder) != Some(editor.folder) {
                bookmarks.move_to(editor.id, editor.folder, usize::MAX)?;
            }
            Ok(())
        });
        if saved.is_none() {
            self.bookmarks.editor = Some(editor);
        }
    }

    pub(crate) fn close_bookmark_editor(&mut self) {
        self.bookmarks.editor = None;
    }

    pub(crate) fn remove_bookmark(&mut self, id: BookmarkId) {
        if self
            .bookmarks
            .editor
            .as_ref()
            .is_some_and(|editor| editor.id == id)
        {
            self.bookmarks.editor = None;
        }
        self.change_bookmarks(|bookmarks| bookmarks.remove(id));
    }

    /// Adds a folder to the end of `parent` and opens the editor to name it.
    pub(crate) fn add_bookmark_folder(&mut self, parent: BookmarkId) {
        if let Some(id) =
            self.change_bookmarks(|bookmarks| bookmarks.add_folder(parent, "New Folder", now()))
        {
            self.edit_bookmark(id, true);
        }
    }

    pub(crate) fn move_bookmark(&mut self, id: BookmarkId, folder: BookmarkId, index: usize) {
        self.change_bookmarks(|bookmarks| bookmarks.move_to(id, folder, index));
    }

    /// Opens a bookmark in the active tab, or in a new one.
    pub(crate) fn open_bookmark(&mut self, id: BookmarkId, new_tab: bool, tx: &mut Tx) {
        let Some(url) = self
            .bookmarks()
            .get(id)
            .and_then(|bookmark| bookmark.url().map(str::to_owned))
        else {
            return;
        };
        self.bookmarks.open_folders.clear();
        self.bookmarks.sidebar_menu = false;
        let active = self.browser.active_tab().map(crate::tab::Tab::id);
        let result = match active.filter(|_| !new_tab && !tx.mods().cmd) {
            Some(tab) => self.navigate_to(tab, &url),
            None => self.browser.open_tab(&url).map(|tab| {
                if !tx.mods().cmd || tx.mods().shift {
                    self.activate(tab);
                }
            }),
        };
        match result {
            Ok(()) => tx.focus(crate::ui::viewport::focus_key(&self.browser), false),
            Err(error) => self.bookmarks.error = Some(error.to_string()),
        }
    }

    /// Opens every link in a folder, each in a new background tab.
    pub(crate) fn open_bookmark_folder(&mut self, id: BookmarkId) {
        let urls: Vec<String> = self
            .bookmarks()
            .get(id)
            .map(|folder| {
                folder
                    .children()
                    .iter()
                    .filter_map(|child| child.url().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        self.bookmarks.open_folders.clear();
        self.bookmarks.sidebar_menu = false;
        for url in urls {
            let _ = self.browser.open_tab(&url);
        }
    }

    /// Opens the bookmark as a new tab among the Favourites.
    pub(crate) fn add_bookmark_to_favourites(&mut self, id: BookmarkId) {
        let Some(url) = self
            .bookmarks()
            .get(id)
            .and_then(|bookmark| bookmark.url().map(str::to_owned))
        else {
            return;
        };
        if let Ok(tab) = self.browser.open_tab(&url) {
            let end = self.browser.section_tabs(TabSection::Favourite).count();
            let _ = self.browser.move_tab_to(tab, TabSection::Favourite, end);
        }
    }

    fn navigate_to(&mut self, tab: TabId, url: &str) -> Result<(), crate::browser::Error> {
        if let Some(route) = elaris_core::internal::InternalRoute::parse(url) {
            return self.browser.navigate_internal(tab, route);
        }
        self.browser.navigate(tab, url)
    }
}

/// Opens the bookmarks page, or goes to the one already open.
pub(crate) fn open_bookmarks(state: &mut AppState, tx: &mut Tx) {
    state.dismiss_find();
    state.omnibox.close();
    let tab = state
        .browser
        .internal_tab(InternalPage::Bookmarks)
        .unwrap_or_else(|| {
            state
                .browser
                .open_tab(InternalPage::Bookmarks.address())
                .expect("valid built-in Bookmarks route")
        });
    state.activate(tab);
    tx.focus(crate::ui::BOOKMARKS_SEARCH, false);
}
