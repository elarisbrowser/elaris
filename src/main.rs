// SPDX-License-Identifier: GPL-3.0-only

mod address;
mod app;
mod appearance;
mod browser;
mod config;
mod engine;
mod input;
mod internal;
#[cfg(target_os = "macos")]
mod menu_macos;
mod menu_state;
mod omnibox;
mod perf;
mod permissions;
mod settings;
mod shortcuts;
mod site_controls;
#[cfg(target_os = "macos")]
mod site_popover_macos;
mod storage;
mod tab;
#[cfg(target_os = "macos")]
mod titlebar_controls;
mod ui;
#[cfg(target_os = "macos")]
mod window_macos;

use std::sync::Arc;

#[cfg(target_os = "linux")]
use rune::raw_window_handle::RawWindowHandle;
use rune::{App, AppSpec, Chrome, WindowOptions};

fn main() {
    let spec = AppSpec {
        // Headless tooling has no native `on_ready`; connect workers there.
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
        .window_options(WindowOptions {
            transparent: true,
            #[cfg(target_os = "linux")]
            borderless: true,
            ..WindowOptions::default()
        })
        // macOS drags the titlebar band natively; the window controls sit centred in it, level
        // with the header's buttons.
        .chrome(Chrome {
            traffic_lights: Some(ui::TRAFFIC_LIGHTS),
            ..Chrome::DEFAULT
        });
    #[cfg(target_os = "macos")]
    let app = app.keep_running().release_gpu_when_closed();
    #[cfg(target_os = "linux")]
    let app = {
        let app_handle = app.handle();
        app.on_window(move |window| {
            let x11_window = window
                .with_raw_handle(|handle| match handle.as_raw() {
                    RawWindowHandle::Xlib(handle) => Some(handle.window),
                    _ => None,
                })
                .ok()
                .flatten();
            app_handle.update(move |state, _| state.titlebar.set_x11_window(x11_window));
        })
    };
    let app_handle = app.handle();
    let app = app.on_ready(move |_| {
        #[cfg(target_os = "macos")]
        menu_macos::install(app_handle.clone());
        let wake_handle = app_handle.clone();
        let waker: rune::Waker = Arc::new(move || wake_handle.wake());
        let app_handle = app_handle.clone();
        app_handle.update(move |state, _| {
            app::start_workers(state, waker);
        });
    });
    #[cfg(target_os = "macos")]
    let app = {
        let app_handle = app.handle();
        app.on_window(move |window| {
            window_macos::install(window);
            titlebar_controls::install(window);
            site_popover_macos::install(window, app_handle.clone());
            if let Some(refresh_rate) = titlebar_controls::refresh_rate(window) {
                app_handle.update(move |state, _| state.set_display_refresh_rate(refresh_rate));
            }
        })
    };
    app.modifier_keys()
        .on_frame_drawn(|_| perf::frame_drawn())
        .on_ime(ui::viewport::ime)
        .on_focus(app::on_window_focus)
        .on_close(app::window_closed)
        .on_quit(app::shut_down)
        .run();
}
