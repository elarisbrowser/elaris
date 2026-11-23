// SPDX-License-Identifier: GPL-3.0-only

//! Download state and requests, independent of Rune and Ladybird's C++ types.

use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

/// A browser-assigned identifier that stays stable for the lifetime of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DownloadId(u64);

impl DownloadId {
    /// The numeric value, useful for logs and persistence.
    pub fn get(self) -> u64 {
        self.0
    }

    /// Reserves a process-unique identifier for a new download.
    ///
    /// # Errors
    /// Returns [`Error::IdExhausted`] when the counter cannot advance without wrapping.
    pub fn next() -> Result<Self, Error> {
        let mut id = NEXT_DOWNLOAD_ID.load(Ordering::Relaxed);
        loop {
            let next = id.checked_add(1).ok_or(Error::IdExhausted)?;
            match NEXT_DOWNLOAD_ID.compare_exchange_weak(
                id,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(Self(id)),
                Err(actual) => id = actual,
            }
        }
    }
}

/// A request to save a URL using Ladybird's browser networking stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The HTTP or HTTPS URL to fetch.
    pub url: String,
    /// An optional name supplied by the caller; Ladybird sanitizes it before saving.
    pub suggested_filename: Option<String>,
}

impl Request {
    /// Creates a request with no caller supplied filename.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            suggested_filename: None,
        }
    }

    /// Sets the filename Ladybird uses if the response does not suggest one.
    pub fn suggested_filename(mut self, filename: impl Into<String>) -> Self {
        self.suggested_filename = Some(filename.into());
        self
    }
}

/// The lifecycle of a download shown by Elaris.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Status {
    /// Elaris queued the request and Ladybird has not confirmed it yet.
    Pending,
    /// Ladybird is receiving and writing response data.
    Downloading,
    /// Stopped part way by the user or a previous shutdown; it can resume.
    Paused,
    /// The completed file is available at [`Download::destination`].
    Completed,
    /// Ladybird could not finish or save the file.
    Failed,
    /// The user or browser stopped the transfer.
    Cancelled,
}

/// A download's current, UI-independent state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    /// The stable browser identifier.
    pub id: DownloadId,
    /// The suggested filename, after Ladybird has sanitized it when available.
    pub filename: String,
    /// The original URL that initiated the download.
    pub source_url: String,
    /// The final destination, once Ladybird has selected one.
    pub destination: Option<PathBuf>,
    /// The current lifecycle state.
    pub status: Status,
    /// Bytes written so far.
    pub bytes_downloaded: u64,
    /// Response length, when the server supplied it.
    pub total_bytes: Option<u64>,
    /// A transfer, file-system, open, or reveal error, if one has occurred.
    pub error: Option<String>,
    /// When this browser first learned of the download, for time-ranged clearing.
    pub started_at: std::time::SystemTime,
    /// Started from a private window, so only private windows list it.
    pub private: bool,
    /// The server supports ranges, so a pause or restart can continue where it stopped.
    pub can_resume: bool,
    /// Ladybird lost the connection and will try again by itself.
    pub retrying: bool,
}

impl Status {
    /// Still transferring or able to continue: not yet part of the download history.
    pub fn is_active(self) -> bool {
        matches!(self, Self::Pending | Self::Downloading | Self::Paused)
    }
}

impl Download {
    /// Creates the initial state before Ladybird confirms a queued request.
    pub fn pending(id: DownloadId, request: &Request, url: String) -> Self {
        let filename = request
            .suggested_filename
            .clone()
            .or_else(|| url.rsplit('/').next().map(str::to_owned))
            .filter(|filename| !filename.is_empty())
            .unwrap_or_else(|| "download".to_owned());
        Self {
            id,
            filename,
            source_url: url,
            destination: None,
            status: Status::Pending,
            bytes_downloaded: 0,
            total_bytes: None,
            error: None,
            started_at: std::time::SystemTime::now(),
            private: false,
            can_resume: false,
            retrying: false,
        }
    }
}

/// A Ladybird snapshot copied across the bridge and assigned a browser identifier by Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Update {
    /// The current suggested or final filename.
    pub filename: String,
    /// The original URL that initiated the download.
    pub source_url: String,
    /// The selected destination, when it can be represented as a native path.
    pub destination: Option<PathBuf>,
    /// The lifecycle state.
    pub status: Status,
    /// Bytes written so far.
    pub bytes_downloaded: u64,
    /// Response length, when known.
    pub total_bytes: Option<u64>,
    /// The transfer error, when Ladybird reports one.
    pub error: Option<String>,
    /// Started from a private window.
    pub private: bool,
    /// The server supports ranges.
    pub can_resume: bool,
    /// Waiting to retry a lost connection.
    pub retrying: bool,
    pub native_id: u64,
}

impl Update {
    /// The Ladybird-side identifier used internally to cancel this transfer.
    pub fn native_id(&self) -> u64 {
        self.native_id
    }

    /// Applies this snapshot to the stable browser identifier assigned by Rust.
    pub fn into_download(self, id: DownloadId) -> Download {
        Download {
            id,
            filename: self.filename,
            source_url: self.source_url,
            destination: self.destination,
            status: self.status,
            bytes_downloaded: self.bytes_downloaded,
            total_bytes: self.total_bytes,
            error: self.error,
            started_at: std::time::SystemTime::now(),
            private: self.private,
            can_resume: self.can_resume,
            retrying: self.retrying,
        }
    }
}

/// An invalid request or an operation that cannot be applied to the current download state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The request is not an HTTP or HTTPS URL.
    InvalidAddress,
    /// The engine is not connected.
    EngineUnavailable,
    /// No download has this identifier.
    NoSuchDownload,
    /// The requested action does not match the download's state.
    InvalidState,
    /// The browser exhausted its 64-bit download identifier space.
    IdExhausted,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidAddress => "download URL must use HTTP or HTTPS",
            Self::EngineUnavailable => "the web engine is unavailable",
            Self::NoSuchDownload => "no download has that identifier",
            Self::InvalidState => "the download is not in a state that allows this action",
            Self::IdExhausted => "download identifiers are exhausted",
        })
    }
}

impl std::error::Error for Error {}
