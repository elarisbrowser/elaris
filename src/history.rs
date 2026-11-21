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
    CREATE INDEX IF NOT EXISTS history_page ON history(last_visited DESC, url DESC);
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

/// Stable position in the latest-visited ordering.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    last_visited: i64,
    url: String,
}

/// A saved URL with the calendar date and time of its latest visit in the local timezone.
#[derive(Debug)]
pub struct PageEntry {
    pub entry: Entry,
    pub date: String,
    pub time: String,
}

impl PageEntry {
    fn cursor(&self) -> Cursor {
        Cursor {
            last_visited: self.entry.last_visited,
            url: self.entry.url.clone(),
        }
    }
}

/// One bounded result window, with local calendar anchors evaluated on the same connection.
#[derive(Debug)]
pub struct Page {
    pub entries: Vec<PageEntry>,
    pub next: Option<Cursor>,
    pub today: String,
    pub yesterday: String,
    /// Unix timestamp in seconds when the relative date labels need refreshing.
    pub next_midnight: u64,
}

const PAGE_SIZE: usize = 40;

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

    /// Reads the next batch in latest-visit order. Search matches literal title and URL substrings.
    pub fn page(&self, query: &str, after: Option<&Cursor>) -> rusqlite::Result<Page> {
        // Separate the initial query so SQLite can seek directly into the index for older pages.
        let boundary = if after.is_some() {
            "AND (last_visited, url) < (?2, ?3)"
        } else {
            ""
        };
        let mut statement = self.connection.prepare_cached(&format!(
            "SELECT url, title, last_visited, visit_count,
                    coalesce(date(last_visited / 1000000, 'unixepoch', 'localtime'), ''),
                    coalesce(strftime('%H:%M', last_visited / 1000000, 'unixepoch', 'localtime'), '')
             FROM history
             WHERE (?1 IS NULL OR url LIKE ?1 ESCAPE '\\' OR title LIKE ?1 ESCAPE '\\')
               {boundary}
             ORDER BY last_visited DESC, url DESC LIMIT {}",
            PAGE_SIZE + 1
        ))?;
        let query = query.trim();
        let pattern = (!query.is_empty()).then(|| format!("%{}%", escape_like(query)));
        let mut rows = match after {
            Some(cursor) => statement.query(params![pattern, cursor.last_visited, cursor.url])?,
            None => statement.query([pattern])?,
        };
        let mut entries = Vec::with_capacity(PAGE_SIZE);
        let mut next = None;
        while let Some(row) = rows.next()? {
            if entries.len() == PAGE_SIZE {
                next = entries.last().map(PageEntry::cursor);
                break;
            }
            entries.push(PageEntry {
                entry: entry_from_row(row)?,
                date: row.get(4)?,
                time: row.get(5)?,
            });
        }
        let (today, yesterday, next_midnight) = self.connection.query_row(
            "SELECT date('now', 'localtime'), date('now', 'localtime', '-1 day'),
                    unixepoch(date('now', 'localtime', '+1 day'), 'utc')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        Ok(Page {
            entries,
            next,
            today,
            yesterday,
            next_midnight,
        })
    }

    /// Removes the entries for every address in `urls` together.
    pub fn remove_all(&self, urls: &[String]) -> rusqlite::Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        {
            let mut statement = transaction.prepare_cached("DELETE FROM history WHERE url = ?1")?;
            for url in urls {
                statement.execute([url])?;
            }
        }
        transaction.commit()
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

    /// Removes entries last visited at or after `since` microseconds since the Unix epoch.
    pub fn clear_since(&self, since: i64) -> rusqlite::Result<()> {
        self.connection
            .execute("DELETE FROM history WHERE last_visited >= ?1", [since])?;
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
    fn clearing_since_a_time_keeps_older_visits() {
        let temp = TempDir::new();
        let history = History::open(&temp.file()).expect("open history");
        history
            .record_visit_at("https://older.example/", "Older", 10)
            .unwrap();
        history
            .record_visit_at("https://newer.example/", "Newer", 20)
            .unwrap();
        history.clear_since(20).unwrap();
        let urls: Vec<_> = history
            .recent(10)
            .unwrap()
            .into_iter()
            .map(|entry| entry.url)
            .collect();
        assert_eq!(urls, ["https://older.example/"]);
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
            "elaris://history",
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
    fn latest_visited_pages_seek_across_ties_and_match_literal_substrings() {
        let temp = TempDir::new();
        let file = temp.file();
        // Create the original schema before the pagination index existed.
        let legacy = Connection::open(&file).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE history (
            url TEXT PRIMARY KEY NOT NULL, title TEXT NOT NULL,
            last_visited INTEGER NOT NULL, visit_count INTEGER NOT NULL CHECK (visit_count > 0)
        );
        INSERT INTO history VALUES ('https://legacy.example/100_percent', 'Saved title', 3, 2);",
            )
            .unwrap();
        drop(legacy);

        let history = History::open(&file).unwrap();
        for i in 0..84 {
            history
                .record_visit_at(
                    &format!("https://docs.example/guide/{i:02}"),
                    "Handbook",
                    10,
                )
                .unwrap();
        }
        history
            .record_visit_at("https://latest.example/", "Latest", 20)
            .unwrap();
        let mut result = history.page("", None).unwrap();
        assert_eq!(result.entries.len(), PAGE_SIZE);
        assert_eq!(result.entries[0].entry.title, "Latest");
        let mut all = Vec::new();
        loop {
            all.extend(result.entries.into_iter().map(|row| row.cursor()));
            let Some(cursor) = result.next else { break };
            result = history.page("", Some(&cursor)).unwrap();
        }
        assert_eq!(all.len(), 86);
        assert!(all.windows(2).all(|pair| {
            (pair[0].last_visited, &pair[0].url) > (pair[1].last_visited, &pair[1].url)
        }));

        for query in ["SAVED TITLE", "legacy.example", "/100_", "_"] {
            let page = history.page(query, None).unwrap();
            assert_eq!(page.entries.len(), 1);
            assert_eq!(page.entries[0].entry.visit_count, 2);
        }
        assert!(history.page("missing", None).unwrap().entries.is_empty());
        let page = history.page("handbook", None).unwrap();
        let older = history.page("handbook", page.next.as_ref()).unwrap();
        assert_eq!(older.entries.len(), PAGE_SIZE);
        assert!(older.next.is_some());
        drop(history);
        assert_eq!(
            History::open(&file)
                .unwrap()
                .page("saved title", None)
                .unwrap()
                .entries
                .len(),
            1
        );
    }

    #[test]
    fn local_visit_times_cross_midnight_and_daylight_saving() {
        // Give SQLite a real OS timezone without changing it in the parallel test process.
        const CHILD: &str = "ELARIS_TEST_HISTORY_TZ";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "history::tests::local_visit_times_cross_midnight_and_daylight_saving",
                ])
                .env(CHILD, "1")
                .env("TZ", "America/New_York")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        let temp = TempDir::new();
        let history = History::open(&temp.file()).unwrap();
        for (i, time) in [
            "2025-03-09 04:59:59",
            "2025-03-09 05:00:00",
            "2025-03-09 06:59:00",
            "2025-03-09 07:00:00",
            "2025-11-02 05:30:00",
            "2025-11-02 06:30:00",
        ]
        .into_iter()
        .enumerate()
        {
            let timestamp: i64 = history
                .connection
                .query_row("SELECT unixepoch(?1) * 1000000", [time], |row| row.get(0))
                .unwrap();
            history
                .record_visit_at(&format!("https://example.org/{i}"), "", timestamp)
                .unwrap();
        }
        let page = history.page("", None).unwrap();
        let local: Vec<_> = page
            .entries
            .iter()
            .rev()
            .map(|row| (row.date.as_str(), row.time.as_str()))
            .collect();
        assert_eq!(
            local,
            [
                ("2025-03-08", "23:59"),
                ("2025-03-09", "00:00"),
                ("2025-03-09", "01:59"),
                ("2025-03-09", "03:00"),
                ("2025-11-02", "01:30"),
                ("2025-11-02", "01:30"),
            ]
        );
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
