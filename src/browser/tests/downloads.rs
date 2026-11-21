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

fn record(browser: &Browser, status: DownloadStatus, can_resume: bool) -> downloads::DownloadId {
    let request = DownloadRequest::new("https://example.com/big.iso").suggested_filename("big.iso");
    let mut download = downloads::Download::pending(
        downloads::DownloadId::next().unwrap(),
        &request,
        request.url.clone(),
    );
    download.status = status;
    download.can_resume = can_resume;
    download.destination = Some(std::path::PathBuf::from("/tmp/big.iso"));
    let id = download.id;
    browser.downloads.lock().unwrap().push(download);
    id
}

#[test]
fn pause_resume_open_and_remove_follow_each_downloads_state() {
    let (browser, fake) = connected();
    let running = record(&browser, DownloadStatus::Downloading, true);
    let fixed = record(&browser, DownloadStatus::Downloading, false);
    let paused = record(&browser, DownloadStatus::Paused, true);
    let done = record(&browser, DownloadStatus::Completed, false);

    assert!(browser.pause_download(running).is_ok());
    assert!(
        browser.pause_download(fixed).is_err(),
        "a server without ranges can't pause"
    );
    assert!(browser.resume_download(paused).is_ok());
    assert!(browser.resume_download(running).is_err());
    assert!(
        browser.cancel_download(paused).is_ok(),
        "a paused download can be cancelled"
    );
    assert!(browser.open_download(done).is_ok());
    assert!(browser.open_download(running).is_err());
    assert_eq!(
        sent(&fake),
        [
            Command::DownloadAction(running, DownloadAction::Pause),
            Command::DownloadAction(paused, DownloadAction::Resume),
            Command::DownloadAction(paused, DownloadAction::Cancel),
            Command::DownloadAction(done, DownloadAction::Open("/tmp/big.iso".into())),
        ]
    );

    assert!(
        browser.remove_download(running).is_err(),
        "active downloads stay listed"
    );
    assert!(browser.remove_download(done).is_ok());
    assert!(
        browser
            .downloads()
            .iter()
            .all(|download| download.id != done)
    );
}

#[test]
fn retry_starts_the_address_again_in_place_of_the_failed_record() {
    let (mut browser, fake) = connected();
    let failed = record(&browser, DownloadStatus::Failed, false);
    let retried = browser
        .retry_download(failed)
        .expect("a failed download can be retried");
    let downloads = browser.downloads();
    assert!(downloads.iter().all(|download| download.id != failed));
    assert_eq!(
        downloads
            .iter()
            .find(|d| d.id == retried)
            .map(|d| d.filename.as_str()),
        Some("big.iso")
    );
    assert!(matches!(
        sent(&fake).as_slice(),
        [Command::StartDownload(id, request)] if *id == retried && request.url == "https://example.com/big.iso"
    ));
    let done = record(&browser, DownloadStatus::Completed, false);
    assert!(browser.retry_download(done).is_err());
}
