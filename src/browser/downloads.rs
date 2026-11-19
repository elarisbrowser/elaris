// SPDX-License-Identifier: GPL-3.0-only

use super::{Browser, Error};
use crate::address::url_for_address;
use crate::engine::{Command, DownloadAction};
use elaris::downloads::{
    self, Download, DownloadId, Request as DownloadRequest, Status as DownloadStatus,
};

impl Browser {
    /// The browser's downloads, including work initiated by a webpage.
    pub(crate) fn downloads(&self) -> &[Download] {
        &self.downloads
    }

    pub(crate) fn set_download_directory(&self, directory: Option<std::path::PathBuf>) {
        if let Some(engine) = &self.engine {
            engine.send(Command::SetDownloadDirectory(directory));
        }
    }

    /// Starts a Ladybird transfer and returns its stable browser identifier immediately.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "The direct Rust start path complements page initiated downloads from Ladybird."
        )
    )]
    pub(crate) fn start_download(
        &mut self,
        mut request: DownloadRequest,
    ) -> Result<DownloadId, Error> {
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
        self.downloads.push(Download::pending(id, &request, url));
        engine.send(Command::StartDownload(id, request));
        Ok(id)
    }

    /// Cancels an active download without waiting for Ladybird's cancellation callback.
    pub(crate) fn cancel_download(&self, id: DownloadId) -> Result<(), Error> {
        let download = self.download(id)?;
        if !matches!(
            download.status,
            DownloadStatus::Pending | DownloadStatus::Downloading
        ) {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Cancel)
    }

    /// Opens a completed file through the operating system's default handler.
    pub(crate) fn open_download(&self, id: DownloadId) -> Result<(), Error> {
        if self.download(id)?.status != DownloadStatus::Completed {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Open)
    }

    /// Opens the completed file's containing folder in the system file manager.
    pub(crate) fn reveal_download(&self, id: DownloadId) -> Result<(), Error> {
        if self.download(id)?.status != DownloadStatus::Completed {
            return Err(Error::Download(downloads::Error::InvalidState));
        }
        self.send_download_action(id, DownloadAction::Reveal)
    }

    fn download(&self, id: DownloadId) -> Result<&Download, Error> {
        self.downloads
            .iter()
            .find(|download| download.id == id)
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
        let Some(download) = self.downloads.iter_mut().find(|download| download.id == id) else {
            self.downloads.push(update.into_download(id));
            return;
        };
        *download = update.into_download(id);
    }
}
