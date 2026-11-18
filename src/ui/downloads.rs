// SPDX-License-Identifier: GPL-3.0-only

//! The compact, sidebar-anchored downloads popover.

use rune::tok::type_;
use rune::{El, LoopSpec, ScrollSpec, col, el, row, text};
use rune_kit::{Placement, Popover};

use crate::app::AppState;
use crate::ui::controls::icon_button;
use crate::ui::theme::{self, Theme};
use elaris::downloads::{Download, DownloadId, Status};

/// Adds the Downloads trigger and its anchored panel to the sidebar.
pub(crate) fn trigger(state: &AppState, theme: &Theme, layer: &mut Vec<El>) -> El {
    let open = state.downloads_open;
    let active = state
        .browser
        .downloads()
        .iter()
        .filter(|download| matches!(download.status, Status::Pending | Status::Downloading))
        .count();
    let mut button = icon_button(
        "downloads-button",
        "Downloads",
        &super::icons::DOWNLOAD,
        theme::ICON_BUTTON,
        theme,
    )
    .on_press(|state: &mut AppState| state.downloads_open = !state.downloads_open);
    if active > 0 {
        let count = if active > 9 {
            "9+".to_owned()
        } else {
            active.to_string()
        };
        let badge_width = if active > 9 { 18.0 } else { 14.0 };
        button = button.child(
            col()
                .key("downloads-active-count")
                .absolute(theme::ICON_BUTTON - badge_width + 2.0, -2.0)
                .size(badge_width, 14.0)
                .center()
                .radius(7.0)
                .fill(theme.icon)
                .child(text(count).font(type_::LABEL).color(theme.window)),
        );
    }
    let kit = theme.kit(theme.overlay_panel);
    Popover::new("downloads-popover")
        .trigger(button)
        .title("Downloads")
        .placement(Placement::TopStart)
        .gutter(6.0)
        .padding(8.0)
        .open(open)
        .on_close(|state: &mut AppState| state.downloads_open = false)
        .child(panel(state, theme))
        .build(&kit, layer)
}

fn panel(state: &AppState, theme: &Theme) -> El {
    let downloads = state.browser.downloads();
    if downloads.is_empty() {
        return col().key("downloads-empty").w(280.0).py(12.0).child(
            text("Files you download appear here.")
                .font(type_::LABEL)
                .color(theme.text_muted),
        );
    }

    let mut list = col()
        .key("downloads-list")
        .w(280.0)
        .max_h(360.0)
        .gap(2.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    for download in downloads.iter().rev() {
        list = list.child(download_row(download, theme));
    }
    list
}

fn download_row(download: &Download, theme: &Theme) -> El {
    let extension = download
        .filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.chars().take(4).collect::<String>().to_uppercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "FILE".to_owned());
    let file_icon = col()
        .key(format!("download-icon-{}", download.id.get()))
        .size(30.0, 30.0)
        .center()
        .radius(8.0)
        .fill(theme.address_field)
        .child(text(extension).font(type_::LABEL).color(theme.text_muted));

    let name = text(download.filename.clone())
        .font(type_::BODY)
        .color(theme.text)
        .single_line()
        .truncate();
    let details = col()
        .key(format!("download-details-{}", download.id.get()))
        .grow(1.0)
        .min_w(0.0)
        .gap(3.0)
        .child(name)
        .child(
            text(status_text(download))
                .font(type_::LABEL)
                .color(theme.text_muted)
                .single_line()
                .truncate(),
        );
    let details = if matches!(download.status, Status::Pending | Status::Downloading) {
        let fill = if let Some(progress) = progress(download) {
            el().h_full().w_frac(progress).radius(2.0).fill(theme.icon)
        } else {
            el().key(format!("download-indeterminate-{}", download.id.get()))
                .h_full()
                .w_frac(0.25)
                .radius(2.0)
                .fill(theme.icon)
                .looping(LoopSpec::slide(1.2, [-1.0, 0.0], [4.0, 0.0]))
        };
        details.child(
            el().key(format!("download-progress-{}", download.id.get()))
                .w_full()
                .h(3.0)
                .clip_soft(0.0)
                .radius(2.0)
                .fill(theme.address_field)
                .child(fill),
        )
    } else {
        details
    };

    let mut actions = row()
        .key(format!("download-actions-{}", download.id.get()))
        .gap(2.0);
    match download.status {
        Status::Pending | Status::Downloading => {
            actions = actions.child(action_button(
                download.id,
                "Cancel download",
                &super::icons::CLOSE,
                theme,
                |browser, id| browser.cancel_download(id),
            ));
        }
        Status::Completed => {
            actions = actions
                .child(action_button(
                    download.id,
                    "Open file",
                    &super::icons::OPEN,
                    theme,
                    |browser, id| browser.open_download(id),
                ))
                .child(action_button(
                    download.id,
                    "Reveal in file manager",
                    &super::icons::FOLDER,
                    theme,
                    |browser, id| browser.reveal_download(id),
                ));
        }
        Status::Failed | Status::Cancelled => {}
        _ => {}
    }

    row()
        .key(format!("download-row-{}", download.id.get()))
        .w_full()
        .min_h(52.0)
        .gap(8.0)
        .center_y()
        .px(4.0)
        .py(5.0)
        .child(file_icon)
        .child(details)
        .child(actions)
}

fn action_button(
    id: DownloadId,
    label: &'static str,
    glyph: &'static rune::Icon,
    theme: &Theme,
    action: impl Fn(&crate::browser::Browser, DownloadId) -> Result<(), crate::browser::Error> + 'static,
) -> El {
    icon_button(
        &format!("download-action-{}-{label}", id.get()),
        label,
        glyph,
        24.0,
        theme,
    )
    .on_press(move |state: &mut AppState| {
        let _ = action(&state.browser, id);
    })
}

fn status_text(download: &Download) -> String {
    match download.status {
        Status::Pending => "Starting…".to_owned(),
        Status::Downloading => match download.total_bytes {
            Some(total) => format!(
                "{} of {}",
                format_bytes(download.bytes_downloaded),
                format_bytes(total)
            ),
            None => format!("{} downloaded", format_bytes(download.bytes_downloaded)),
        },
        Status::Completed => download
            .error
            .clone()
            .unwrap_or_else(|| "Completed".to_owned()),
        Status::Failed => format!(
            "Failed · {}",
            download.error.as_deref().unwrap_or("unknown error")
        ),
        Status::Cancelled => "Cancelled".to_owned(),
        _ => "Download status unavailable".to_owned(),
    }
}

fn progress(download: &Download) -> Option<f32> {
    if !matches!(download.status, Status::Pending | Status::Downloading) {
        return None;
    }
    let total = download.total_bytes.filter(|total| *total > 0)?;
    Some((download.bytes_downloaded as f64 / total as f64).clamp(0.0, 1.0) as f32)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut amount = bytes as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit < UNITS.len() - 1 {
        amount /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{amount:.1} {}", UNITS[unit])
    }
}
