// SPDX-License-Identifier: GPL-3.0-only

//! Finished downloads kept across restarts. Ladybird itself restores only resumable unfinished
//! downloads; this keeps the list a user saw: completed, failed and cancelled records from
//! ordinary windows. Private downloads are never written.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::downloads::{Download, DownloadId, Request, Status};
use serde::{Deserialize, Serialize};

/// The most records kept; older ones fall off.
const LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Record {
    filename: String,
    source_url: String,
    destination: Option<PathBuf>,
    status: RecordStatus,
    bytes_downloaded: u64,
    total_bytes: Option<u64>,
    error: Option<String>,
    started_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RecordStatus {
    Completed,
    Failed,
    Cancelled,
}

/// The records to keep from `downloads`, newest last, as the JSON the storage worker writes.
pub fn encode(downloads: &[Download]) -> String {
    let records: Vec<Record> = downloads
        .iter()
        .filter(|download| !download.private)
        .filter_map(record)
        .collect();
    let start = records.len().saturating_sub(LIMIT);
    serde_json::to_string(&records[start..]).unwrap_or_else(|_| "[]".to_owned())
}

/// Reads the kept records as finished downloads with fresh ids. A missing file is no history.
pub fn load(path: &Path) -> io::Result<Vec<Download>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let records: Vec<Record> = serde_json::from_str(&contents).map_err(io::Error::other)?;
    Ok(records.into_iter().filter_map(download).collect())
}

fn record(download: &Download) -> Option<Record> {
    let status = match download.status {
        Status::Completed => RecordStatus::Completed,
        Status::Failed => RecordStatus::Failed,
        Status::Cancelled => RecordStatus::Cancelled,
        _ => return None,
    };
    Some(Record {
        filename: download.filename.clone(),
        source_url: download.source_url.clone(),
        destination: download.destination.clone(),
        status,
        bytes_downloaded: download.bytes_downloaded,
        total_bytes: download.total_bytes,
        error: download.error.clone(),
        started_at: download
            .started_at
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
    })
}

fn download(record: Record) -> Option<Download> {
    let id = DownloadId::next().ok()?;
    let mut download = Download::pending(id, &Request::new(&record.source_url), record.source_url);
    download.filename = record.filename;
    download.destination = record.destination;
    download.status = match record.status {
        RecordStatus::Completed => Status::Completed,
        RecordStatus::Failed => Status::Failed,
        RecordStatus::Cancelled => Status::Cancelled,
    };
    download.bytes_downloaded = record.bytes_downloaded;
    download.total_bytes = record.total_bytes;
    download.error = record.error;
    download.started_at = UNIX_EPOCH + Duration::from_secs(record.started_at);
    Some(download)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(status: Status, private: bool) -> Download {
        let request = Request::new("https://example.com/a.zip");
        let mut download =
            Download::pending(DownloadId::next().unwrap(), &request, request.url.clone());
        download.status = status;
        download.private = private;
        download.destination = Some(PathBuf::from("/tmp/a.zip"));
        download.started_at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        download
    }

    #[test]
    fn only_finished_ordinary_downloads_round_trip() {
        let downloads = [
            finished(Status::Completed, false),
            finished(Status::Downloading, false),
            finished(Status::Paused, false),
            finished(Status::Failed, true),
            finished(Status::Cancelled, false),
        ];
        let path = std::env::temp_dir().join(format!(
            "elaris-download-history-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, encode(&downloads)).unwrap();
        let loaded = load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let statuses: Vec<Status> = loaded.iter().map(|download| download.status).collect();
        assert_eq!(statuses, [Status::Completed, Status::Cancelled]);
        assert_eq!(
            loaded[0].destination.as_deref(),
            Some(Path::new("/tmp/a.zip"))
        );
        assert_eq!(loaded[0].started_at, downloads[0].started_at);
        assert_ne!(
            loaded[0].id, downloads[0].id,
            "restored records get fresh ids"
        );
    }

    #[test]
    fn a_missing_file_is_an_empty_history() {
        let path = std::env::temp_dir().join("elaris-download-history-missing.json");
        assert!(load(&path).unwrap().is_empty());
    }
}
