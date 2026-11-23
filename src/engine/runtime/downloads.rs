// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;
use std::process::Command as ProcessCommand;

use super::{Ladybird, Runtime};
use crate::engine::DownloadAction;
use elaris_core::downloads::{DownloadId, Request as DownloadRequest};

impl Runtime {
    pub(super) fn start_download(
        &mut self,
        id: DownloadId,
        request: DownloadRequest,
    ) -> Result<(), String> {
        let started = match &request.destination {
            Some(destination) => self.ladybird.start_download_to(&request.url, destination),
            None => self
                .ladybird
                .start_download(&request.url, request.suggested_filename.as_deref()),
        };
        match started {
            Ok(native_id) => {
                self.native_download_ids.insert(native_id, id);
                self.native_ids_by_download.insert(id, native_id);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    pub(super) fn download_action(
        &mut self,
        id: DownloadId,
        action: DownloadAction,
    ) -> Result<(), String> {
        match action {
            DownloadAction::Cancel => {
                let Some(native_id) = self.native_ids_by_download.get(&id).copied() else {
                    // A cancel queued behind a request that failed to start is already satisfied.
                    return Ok(());
                };
                self.ladybird.cancel_download(native_id)
            }
            DownloadAction::Pause | DownloadAction::Resume => {
                let Some(native_id) = self.native_ids_by_download.get(&id).copied() else {
                    return Err("download is not running in this session".to_owned());
                };
                self.ladybird
                    .pause_download(native_id, action == DownloadAction::Pause)
            }
            // The browser checked the record; a restored history entry has no engine state.
            DownloadAction::Open(path) => open_download(&path, false),
            DownloadAction::Reveal(path) => open_download(&path, true),
        }
    }
}

pub(super) fn configure_download_directory(
    ladybird: &mut Ladybird,
    path: Option<&Path>,
) -> Result<(), String> {
    let Some(path) = path else {
        return ladybird.clear_download_directory();
    };
    if !path.is_absolute() {
        return Err("download directory must be an absolute path".to_owned());
    }
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "cannot create download directory {}: {error}",
            crate::storage::display_path(path)
        )
    })?;
    ladybird.set_download_directory(path)
}

fn open_download(path: &std::path::Path, reveal: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = ProcessCommand::new("open");
        if reveal {
            command.arg("-R");
        }
        command.arg(path);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = ProcessCommand::new("xdg-open");
        command.arg(if reveal {
            path.parent().unwrap_or(path)
        } else {
            path
        });
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open download: {error}"))
}
