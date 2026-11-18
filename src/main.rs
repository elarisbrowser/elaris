// SPDX-License-Identifier: GPL-3.0-only

mod address;
mod app;
mod appearance;
mod browser;
mod engine;
mod input;
mod omnibox;
mod perf;
mod shortcuts;
mod tab;
#[cfg(target_os = "macos")]
mod titlebar_controls;
mod ui;

use rune::{App, AppSpec, Chrome};

fn main() {
    let spec = AppSpec {
        setup: Some(app::setup),
        poll: Some(app::poll),
        on_key: Some(shortcuts::on_key),
        min_size: Some(ui::WINDOW_MIN_SIZE),
        ..AppSpec::new(
            app::APP_NAME,
            (960.0, 640.0),
            app::AppState::default,
            ui::view,
        )
    };
    let app = App::new(spec)
        // macOS drags the titlebar band natively; the window controls sit centred in it, level
        // with the header's buttons.
        .chrome(Chrome {
            traffic_lights: Some(ui::TRAFFIC_LIGHTS),
            ..Chrome::DEFAULT
        });
    #[cfg(target_os = "macos")]
    let app = app.on_window(titlebar_controls::install);
    app.modifier_keys()
        .on_frame_drawn(|_| perf::frame_drawn())
        .on_ime(ui::viewport::ime)
        .on_focus(app::on_window_focus)
        .on_close(app::shut_down)
        .on_quit(app::shut_down)
        .run();
}
