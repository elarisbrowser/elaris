// SPDX-License-Identifier: GPL-3.0-only

//! Local browsing history, stored in a small SQLite database.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(test)]
use rusqlite::OptionalExtension;
use rusqlite::{Connection, params};

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS history (
        url TEXT PRIMARY KEY NOT NULL,
        title TEXT NOT NULL,
        last_visited INTEGER NOT NULL,
        visit_count INTEGER NOT NULL CHECK (visit_count > 0)
    );
    CREATE INDEX IF NOT EXISTS history_recent ON history(last_visited DESC);
    CREATE INDEX IF NOT EXISTS history_frequent ON history(visit_count DESC, last_visited DESC);
";

/// One locally recorded page address and its latest visit details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The committed HTTP or HTTPS address.
    pub url: String,
    /// The latest page title, or the URL when no title was reported.
    pub title: String,
    /// The latest visit time as microseconds since the Unix epoch.
    pub last_visited: i64,
    /// How many committed visits were recorded for this URL.
    pub visit_count: u64,
}

/// One connection, owned by the storage worker or a caller doing synchronous history work.
pub struct History {
    connection: Connection,
}

impl History {
    /// Opens or creates a history database at `path`.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(SCHEMA)?;
        Ok(Self { connection })
    }

    /// Records a committed HTTP or HTTPS navigation. Returns `false` for internal or invalid URLs.
    pub fn record_visit(&self, url: &str, title: &str) -> rusqlite::Result<bool> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros()
            .min(i64::MAX as u128) as i64;
        self.record_visit_at(url, title, now)
    }

    fn record_visit_at(&self, url: &str, title: &str, last_visited: i64) -> rusqlite::Result<bool> {
        if !is_history_url(url) {
            return Ok(false);
        }
        let title = if title.is_empty() { url } else { title };
        self.connection.execute(
            "INSERT INTO history (url, title, last_visited, visit_count)
             VALUES (?1, ?2, ?3, 1)
             ON CONFLICT(url) DO UPDATE SET
                 title = CASE WHEN excluded.title = '' THEN history.title ELSE excluded.title END,
                 last_visited = excluded.last_visited,
                 visit_count = history.visit_count + 1",
            params![url, title, last_visited],
        )?;
        Ok(true)
    }

    /// Updates the saved title for a previously visited URL.
    pub fn update_title(&self, url: &str, title: &str) -> rusqlite::Result<()> {
        if !title.is_empty() && is_history_url(url) {
            self.connection.execute(
                "UPDATE history SET title = ?1 WHERE url = ?2",
                params![title, url],
            )?;
        }
        Ok(())
    }

    /// Returns entries ordered from most recently visited to least recently visited.
    pub fn recent(&self, limit: usize) -> rusqlite::Result<Vec<Entry>> {
        self.query(
            "SELECT url, title, last_visited, visit_count FROM history
             ORDER BY last_visited DESC, visit_count DESC LIMIT ?1",
            limit,
        )
    }

    /// Returns entries ordered by visit count, then by most recent visit.
    pub fn frequent(&self, limit: usize) -> rusqlite::Result<Vec<Entry>> {
        self.query(
            "SELECT url, title, last_visited, visit_count FROM history
             ORDER BY visit_count DESC, last_visited DESC LIMIT ?1",
            limit,
        )
    }

    /// Finds local entries whose title or URL contains `query`.
    pub fn search(&self, query: &str, limit: usize) -> rusqlite::Result<Vec<Entry>> {
        let pattern = format!("%{}%", escape_like(query.trim()));
        let mut statement = self.connection.prepare(
            "SELECT url, title, last_visited, visit_count FROM history
             WHERE lower(url) LIKE lower(?1) ESCAPE '\\'
                OR lower(title) LIKE lower(?1) ESCAPE '\\'
             ORDER BY visit_count DESC, last_visited DESC LIMIT ?2",
        )?;
        let rows = statement.query_map(params![pattern, limit as i64], entry_from_row)?;
        rows.collect()
    }

    /// Removes the entry for `url`, returning whether a row was deleted.
    pub fn remove(&self, url: &str) -> rusqlite::Result<bool> {
        Ok(self
            .connection
            .execute("DELETE FROM history WHERE url = ?1", [url])?
            > 0)
    }

    /// Removes every saved history entry.
    pub fn clear(&self) -> rusqlite::Result<()> {
        self.connection.execute("DELETE FROM history", [])?;
        Ok(())
    }

    fn query(&self, sql: &str, limit: usize) -> rusqlite::Result<Vec<Entry>> {
        let mut statement = self.connection.prepare(sql)?;
        let rows = statement.query_map([limit as i64], entry_from_row)?;
        rows.collect()
    }

    #[cfg(test)]
    fn entry(&self, url: &str) -> rusqlite::Result<Option<Entry>> {
        self.connection
            .query_row(
                "SELECT url, title, last_visited, visit_count FROM history WHERE url = ?1",
                [url],
                entry_from_row,
            )
            .optional()
    }
}

fn entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Entry> {
    Ok(Entry {
        url: row.get(0)?,
        title: row.get(1)?,
        last_visited: row.get(2)?,
        visit_count: row.get(3)?,
    })
}

fn escape_like(query: &str) -> String {
    query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn is_history_url(url: &str) -> bool {
    if url
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return false;
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    !authority.is_empty() && !authority.chars().any(char::is_whitespace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("elaris-history-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create temporary history directory");
            Self(path)
        }

        fn file(&self) -> std::path::PathBuf {
            self.0.join("history.sqlite")
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn repeated_visits_update_title_and_frequency() {
        let temp = TempDir::new();
        let history = History::open(&temp.file()).expect("open history");
        assert!(
            history
                .record_visit("https://example.com/", "")
                .expect("record visit")
        );
        assert!(
            history
                .record_visit("https://example.com/", "Example")
                .expect("record repeated visit")
        );
        let entry = history
            .entry("https://example.com/")
            .expect("read entry")
            .expect("entry exists");
        assert_eq!(entry.title, "Example");
        assert_eq!(entry.visit_count, 2);
    }

    #[test]
    fn title_updates_and_recent_ranking_follow_committed_visits() {
        let temp = TempDir::new();
        let history = History::open(&temp.file()).expect("open history");
        history
            .record_visit_at("https://older.example/", "Older", 10)
            .unwrap();
        history
            .record_visit_at("https://newer.example/", "", 20)
            .unwrap();
        history
            .update_title("https://newer.example/", "Newer page")
            .unwrap();

        let recent = history.recent(2).unwrap();
        assert_eq!(recent[0].url, "https://newer.example/");
        assert_eq!(recent[0].title, "Newer page");
        assert_eq!(recent[1].url, "https://older.example/");
    }

    #[test]
    fn internal_and_invalid_urls_are_not_recorded() {
        let temp = TempDir::new();
        let history = History::open(&temp.file()).expect("open history");
        for url in [
            "about:blank",
            "data:text/plain,private",
            "https://",
            "https://bad host/",
        ] {
            assert!(!history.record_visit(url, "").expect("filter URL"));
        }
        assert!(history.recent(10).expect("read history").is_empty());
    }

    #[test]
    fn recent_frequent_search_and_deletion_use_stored_visits() {
        let temp = TempDir::new();
        let history = History::open(&temp.file()).expect("open history");
        history
            .record_visit("https://github.com/rust-lang", "GitHub Rust")
            .unwrap();
        history
            .record_visit("https://github.com/rust-lang", "GitHub Rust")
            .unwrap();
        history
            .record_visit("https://example.com/", "Example Site")
            .unwrap();

        assert_eq!(
            history.frequent(1).unwrap()[0].url,
            "https://github.com/rust-lang"
        );
        assert_eq!(history.search("github", 10).unwrap()[0].visit_count, 2);
        assert_eq!(history.recent(1).unwrap().len(), 1);
        assert!(history.remove("https://github.com/rust-lang").unwrap());
        assert!(history.search("github", 10).unwrap().is_empty());
        history.clear().unwrap();
        assert!(history.recent(10).unwrap().is_empty());
    }

    #[test]
    fn search_escapes_sql_like_wildcards() {
        let temp = TempDir::new();
        let history = History::open(&temp.file()).expect("open history");
        history
            .record_visit("https://example.com/100_percent", "Percent % path")
            .unwrap();
        history
            .record_visit("https://example.com/abc", "Other")
            .unwrap();
        let matches = history.search("%", 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].title, "Percent % path");
    }
}
