// SPDX-License-Identifier: GPL-3.0-only

mod address;
mod app;
mod appearance;
mod browser;
mod config;
#[cfg(target_os = "macos")]
mod drag_overlay_macos;
mod engine;
mod input;
mod internal;
#[cfg(target_os = "macos")]
mod menu_macos;
#[cfg(target_os = "macos")]
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
use rune::{App, AppSpec, Chrome, WindowOptions, WindowSpec};

fn main() {
    let app = configure_app(App::new(app_spec(app::AppState::default)));
    #[cfg(target_os = "macos")]
    let app = {
        let mut app = app.keep_running().release_gpu_when_closed();
        drag_overlay_macos::install(&mut app);
        app
    };
    let app_handle = app.handle();
    let app = app.on_ready(move |_| {
        #[cfg(target_os = "macos")]
        menu_macos::install(app_handle.clone());
        start_window_workers(app_handle.clone());
    });
    app.run();
}

fn app_spec(init: fn() -> app::AppState) -> AppSpec<app::AppState> {
    AppSpec {
        // Headless tooling has no native `on_ready`; connect workers there.
        setup: Some(app::setup_headless),
        poll: Some(app::poll),
        on_key: Some(shortcuts::on_key),
        min_size: Some(ui::WINDOW_MIN_SIZE),
        ..AppSpec::new(app::APP_NAME, (960.0, 640.0), init, ui::view)
    }
}

fn configure_app(app: App<app::AppState>) -> App<app::AppState> {
    let app = app
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
    #[cfg(target_os = "linux")]
    let app = {
        let app_handle = app.handle();
        app.on_window(move |window| {
            let retained_window = window.clone();
            let register_handle = app_handle.clone();
            let x11_window = window
                .with_raw_handle(|handle| match handle.as_raw() {
                    RawWindowHandle::Xlib(handle) => Some(handle.window),
                    _ => None,
                })
                .ok()
                .flatten();
            register_handle.update(move |state, _| {
                state.titlebar.set_x11_window(x11_window);
                app::register_window(state.window_id, retained_window);
            });
        })
    };
    #[cfg(target_os = "macos")]
    let app = {
        let app_handle = app.handle();
        app.on_window(move |window| {
            let retained_window = window.clone();
            let register_handle = app_handle.clone();
            register_handle.update(move |state, _| {
                app::register_window(state.window_id, retained_window);
            });
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
        .on_reopen(app::on_reopen)
}

fn start_window_workers(handle: rune::AppHandle<app::AppState>) {
    let wake_handle = handle.clone();
    let waker: rune::Waker = Arc::new(move || wake_handle.wake());
    #[cfg(target_os = "macos")]
    let menu_handle = handle.clone();
    handle.update(move |state, _| {
        app::start_workers(state, waker);
        let restored = state.take_restored_windows();
        #[cfg(target_os = "macos")]
        menu_macos::register_window(state.window_id, menu_handle);
        for session in restored {
            open_browser_window_with_session(session);
        }
    });
}

/// Queues a full browser window in Rune's current event loop turn.
pub(crate) fn open_browser_window() {
    queue_browser_window(app::AppState::new_window(), None);
}

fn open_browser_window_with_session(session: storage::Session) {
    queue_browser_window(app::AppState::new_window_from_session(session), None);
}

fn queue_browser_window(
    state: app::AppState,
    placement: Option<app::detach::Placement>,
) -> (rune::WindowHandle, rune::AppHandle<app::AppState>) {
    let key = format!("elaris-window-{}", state.window_id);
    let window_id = state.window_id;
    let app = configure_app(App::new(app_spec(app::AppState::new_window)).init(move || state));
    #[cfg(target_os = "macos")]
    let app = app.keep_running().release_gpu_when_closed();
    let handle = app.handle();
    let workers = handle.clone();
    let mut placement = placement;
    let app = app.on_ready(move |window| {
        if let Some(placement) = placement.take() {
            placement.apply(window);
        }
        start_window_workers(workers.clone());
    });
    let native = rune::open_window(WindowSpec::from_app(key, app));
    app::register_window(window_id, native.clone());
    native.open();
    (native, handle)
}
