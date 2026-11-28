// SPDX-License-Identifier: GPL-3.0-only

use super::{Browser, Error};
use crate::engine::{Command, DownloadAction};
use crate::tab::TabId;
use elaris_core::address::url_for_address;
use elaris_core::downloads::{
    self, Download, DownloadId, Request as DownloadRequest, Status as DownloadStatus,
};

impl Browser {
    /// This window's downloads, including work initiated by a webpage. Private and ordinary
    /// windows each see only their own session's.
    pub(crate) fn downloads(&self) -> Vec<Download> {
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|download| download.private == self.private)
            .cloned()
            .collect()
    }

    /// Compares this window's downloads without cloning rows on an unchanged poll.
    pub(super) fn sync_download_snapshot(&mut self) -> bool {
        let downloads = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let visible = downloads
            .iter()
            .filter(|download| download.private == self.private);
        if visible.clone().eq(&self.last_download_snapshot) {
            return false;
        }
        self.last_download_snapshot = visible.cloned().collect();
        true
    }

    pub(crate) fn active_download_count(&self) -> usize {
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .filter(|download| {
                download.private == self.private
                    && matches!(
                        download.status,
                        DownloadStatus::Pending | DownloadStatus::Downloading
                    )
            })
            .count()
    }

    /// Every download in the process, private ones too, for the history to filter.
    pub(crate) fn all_downloads(&self) -> Vec<Download> {
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Forgets private downloads once the private session they belonged to has ended.
    pub(crate) fn forget_private_downloads(&self) {
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|download| !download.private);
    }

    #[cfg(not(test))]
    pub(crate) fn share_downloads(
        &mut self,
        downloads: std::sync::Arc<std::sync::Mutex<Vec<Download>>>,
    ) {
        self.downloads = downloads;
    }

    pub(crate) fn set_download_directory(&self, directory: Option<std::path::PathBuf>) {
        if let Some(engine) = &self.engine {
            engine.send(Command::SetDownloadDirectory(directory));
        }
    }

    /// Lists a file the engine saved for tab `id`, such as a screenshot, among the downloads.
    pub(crate) fn add_saved_file(&self, id: TabId, path: Result<std::path::PathBuf, String>) {
        let Ok(download_id) = DownloadId::next() else {
            return;
        };
        let source = self
            .page(id)
            .map_or_else(String::new, |tab| tab.url.clone());
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Download::saved(download_id, source, path, self.private));
    }

    /// Starts a Ladybird transfer and returns its stable browser identifier immediately.
    pub(crate) fn start_download(
        &mut self,
        mut request: DownloadRequest,
    ) -> Result<DownloadId, Error> {
        // The browser-level start command currently targets the ordinary engine session.
        if self.private {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        let Some(engine) = &self.engine else {
            return Err(Error::Download(downloads::Error::EngineUnavailable));
        };
        let is_http_url = request
            .url
            .get(..7)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("http://"))
            || request
                .url
                .get(..8)
                .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"));
        if !is_http_url {
            return Err(Error::Download(downloads::Error::InvalidAddress));
        }
        let url = url_for_address(&request.url)
            .map_err(|_| Error::Download(downloads::Error::InvalidAddress))?;
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(Error::Download(downloads::Error::InvalidAddress));
        }
        request.url.clone_from(&url);
        let id = DownloadId::next().map_err(Error::Download)?;
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(Download::pending(id, &request, url));
        engine.send(Command::StartDownload(id, request));
        Ok(id)
    }

    /// Cancels an active or paused download without waiting for Ladybird's callback.
    pub(crate) fn cancel_download(&self, id: DownloadId) -> Result<(), Error> {
        if !self.download(id)?.status.is_active() {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Cancel)
    }

    /// Pauses a running download the server lets resume.
    pub(crate) fn pause_download(&self, id: DownloadId) -> Result<(), Error> {
        let download = self.download(id)?;
        if download.status != DownloadStatus::Downloading || !download.can_resume {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Pause)
    }

    /// Continues a paused download where it stopped.
    pub(crate) fn resume_download(&self, id: DownloadId) -> Result<(), Error> {
        if self.download(id)?.status != DownloadStatus::Paused {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Resume)
    }

    /// Starts a failed or cancelled download again from its address, in place of the old record.
    pub(crate) fn retry_download(&mut self, id: DownloadId) -> Result<DownloadId, Error> {
        let download = self.download(id)?;
        if !matches!(
            download.status,
            DownloadStatus::Failed | DownloadStatus::Cancelled
        ) {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        let request =
            DownloadRequest::new(download.source_url).suggested_filename(download.filename);
        let retried = self.start_download(request)?;
        self.remove_download(id)?;
        Ok(retried)
    }

    /// Forgets a finished download. The file itself stays.
    pub(crate) fn remove_download(&self, id: DownloadId) -> Result<(), Error> {
        if self.download(id)?.status.is_active() {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|download| download.id != id);
        Ok(())
    }

    /// Opens a completed file through the operating system's default handler.
    pub(crate) fn open_download(&self, id: DownloadId) -> Result<(), Error> {
        let path = self.completed_path(id)?;
        self.send_download_action(id, DownloadAction::Open(path))
    }

    /// Opens the completed file's containing folder in the system file manager.
    pub(crate) fn reveal_download(&self, id: DownloadId) -> Result<(), Error> {
        let path = self.completed_path(id)?;
        self.send_download_action(id, DownloadAction::Reveal(path))
    }

    fn completed_path(&self, id: DownloadId) -> Result<std::path::PathBuf, Error> {
        let download = self.download(id)?;
        match (download.status, download.destination) {
            (DownloadStatus::Completed, Some(path)) => Ok(path),
            _ => Err(Error::Download(downloads::Error::InvalidState)),
        }
    }

    fn download(&self, id: DownloadId) -> Result<Download, Error> {
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|download| download.id == id && download.private == self.private)
            .cloned()
            .ok_or(Error::Download(downloads::Error::NoSuchDownload))
    }

    fn send_download_action(&self, id: DownloadId, action: DownloadAction) -> Result<(), Error> {
        let Some(engine) = &self.engine else {
            return Err(Error::Download(downloads::Error::EngineUnavailable));
        };
        engine.send(Command::DownloadAction(id, action));
        Ok(())
    }

    pub(super) fn apply_download_update(&mut self, id: DownloadId, update: downloads::Update) {
        let mut downloads = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(download) = downloads.iter_mut().find(|download| download.id == id) else {
            // A private session's reset reports its downloads cancelled after they were forgotten.
            let finished_private = update.private
                && !matches!(
                    update.status,
                    DownloadStatus::Pending | DownloadStatus::Downloading
                );
            if !finished_private {
                downloads.push(update.into_download(id));
            }
            return;
        };
        let started_at = download.started_at;
        *download = update.into_download(id);
        download.started_at = started_at;
    }

    /// Forgets finished downloads that started at or after `since`, or all of them.
    pub(crate) fn remove_finished_downloads(&self, since: Option<std::time::SystemTime>) {
        self.downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .retain(|download| {
                matches!(
                    download.status,
                    DownloadStatus::Pending | DownloadStatus::Downloading
                ) || since.is_some_and(|since| download.started_at < since)
            });
    }

    pub(crate) fn clear_browsing_data(
        &self,
        since: Option<std::time::SystemTime>,
        data: elaris::ladybird::BrowsingData,
    ) {
        if data.download_history {
            self.remove_finished_downloads(since);
        }
        let since_ms = since.map(|since| {
            since
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(i64::MAX as u128) as i64
        });
        if let Some(engine) = &self.engine {
            engine.send(Command::ClearBrowsingData(since_ms, data));
        }
    }

    pub(super) fn fail_download_start(&mut self, id: DownloadId, error: String) {
        if let Some(download) = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter_mut()
            .find(|item| item.id == id)
        {
            download.status = DownloadStatus::Failed;
            download.error = Some(error);
        }
    }

    pub(super) fn set_download_error(&mut self, id: DownloadId, error: String) {
        if let Some(download) = self
            .downloads
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter_mut()
            .find(|item| item.id == id)
        {
            download.error = Some(error);
        }
    }
}
