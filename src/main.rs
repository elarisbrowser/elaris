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

use std::sync::Arc;

use rune::{App, AppSpec, Chrome};

fn main() {
    let spec = AppSpec {
        // Headless tooling has no native `on_ready`; keep engine startup there.
        setup: Some(app::setup_headless),
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
    let app_handle = app.handle();
    let mut engine_started = false;
    let app = app.on_ready(move |_| {
        if engine_started {
            return;
        }
        engine_started = true;

        let wake_handle = app_handle.clone();
        let waker: rune::Waker = Arc::new(move || wake_handle.wake());
        let app_handle = app_handle.clone();
        app_handle.update(move |state, _| app::start_engine(state, waker));
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
