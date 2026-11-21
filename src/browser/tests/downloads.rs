// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{DownloadAction, Update};
use elaris::downloads::{self, Request as DownloadRequest, Status as DownloadStatus};

#[test]
fn download_directory_updates_are_engine_commands_and_report_without_failing_the_browser() {
    let (mut browser, fake) = connected();
    let path = std::path::PathBuf::from("/tmp/elaris-downloads");
    browser.set_download_directory(Some(path.clone()));
    assert_eq!(sent(&fake), [Command::SetDownloadDirectory(Some(path))]);

    fake.updates
        .send(Update::DownloadDirectoryFailed(
            "directory is read-only".into(),
        ))
        .unwrap();
    let polled = browser.poll();
    assert!(polled.changed);
    assert_eq!(
        polled.download_directory_error.as_deref(),
        Some("directory is read-only")
    );
    assert!(browser.engine_error.is_none());

    browser.set_download_directory(None);
    assert_eq!(sent(&fake), [Command::SetDownloadDirectory(None)]);
}

#[test]
fn downloads_are_queued_with_a_stable_id_and_can_be_cancelled() {
    let (mut browser, fake) = connected();
    let id = browser
        .start_download(DownloadRequest::new("https://example.com/archive.zip"))
        .expect("an HTTP download can be queued");

    let downloads = browser.downloads();
    let download = downloads
        .iter()
        .find(|download| download.id == id)
        .expect("queued download is visible immediately");
    assert_eq!(download.status, DownloadStatus::Pending);
    assert_eq!(download.source_url, "https://example.com/archive.zip");
    assert_eq!(download.filename, "archive.zip");
    assert_eq!(
        fake.commands.recv().expect("start command is queued"),
        Command::StartDownload(id, DownloadRequest::new("https://example.com/archive.zip"))
    );

    browser
        .cancel_download(id)
        .expect("pending download can be cancelled");
    assert_eq!(
        fake.commands.recv().expect("cancel command is queued"),
        Command::DownloadAction(id, DownloadAction::Cancel)
    );
}

#[test]
fn downloads_reject_non_http_addresses_and_unavailable_engines() {
    let mut disconnected = Browser::default();
    assert_eq!(
        disconnected.start_download(DownloadRequest::new("https://example.com")),
        Err(Error::Download(downloads::Error::EngineUnavailable))
    );

    let (mut browser, _) = connected();
    assert_eq!(
        browser.start_download(DownloadRequest::new("file:///etc/passwd")),
        Err(Error::Download(downloads::Error::InvalidAddress))
    );
}

#[test]
fn private_and_ordinary_windows_each_list_only_their_own_downloads() {
    let mut browser = Browser::default();
    let download = |private| downloads::Download {
        private,
        ..downloads::Download::pending(
            downloads::DownloadId::next().unwrap(),
            &DownloadRequest::new("https://example.com/file.zip"),
            "https://example.com/file.zip".into(),
        )
    };
    browser
        .downloads
        .lock()
        .unwrap()
        .extend([download(false), download(true)]);
    assert!(browser.downloads().iter().all(|d| !d.private) && browser.downloads().len() == 1);
    browser.private = true;
    assert!(browser.downloads().iter().all(|d| d.private) && browser.downloads().len() == 1);
    browser.forget_private_downloads();
    assert!(browser.downloads().is_empty());
    browser.private = false;
    assert_eq!(browser.downloads().len(), 1, "ordinary downloads stay");
}
