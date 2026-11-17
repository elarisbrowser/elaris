// SPDX-License-Identifier: GPL-3.0-only

mod address;
mod app;
mod appearance;
mod browser;
mod engine;
mod input;
mod perf;
mod shortcuts;
mod tab;
#[cfg(target_os = "macos")]
mod titlebar_controls;
mod ui;

use rune::{App, AppSpec, Titlebar};

fn main() {
    let spec = AppSpec {
        setup: Some(app::setup),
        poll: Some(app::poll),
        on_key: Some(app::on_key),
        min_size: Some(ui::WINDOW_MIN_SIZE),
        ..AppSpec::new(
            app::APP_NAME,
            (960.0, 640.0),
            app::AppState::default,
            ui::view,
        )
    };
    App::new(spec)
        // The sidebar's header is the titlebar, so the window controls centre on its buttons.
        .titlebar(Titlebar::new(ui::HEADER_HEIGHT))
        .on_window(titlebar_controls::install)
        .on_frame_drawn(|_| perf::frame_drawn())
        .on_focus(app::on_window_focus)
        .on_close(app::shut_down)
        .on_quit(app::shut_down)
        .run();
}
