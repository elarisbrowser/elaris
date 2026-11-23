// SPDX-License-Identifier: GPL-3.0-only

mod address;
mod app;
mod appearance;
mod browser;
mod config;
mod dev;
mod download_history;
mod engine;
mod input;
mod internal;
mod omnibox;
mod page_accessibility;
mod perf;
mod permissions;
mod platform;
mod settings;
mod shortcuts;
mod site_controls;
mod storage;
mod tab;
mod ui;

use std::sync::Arc;

use rune::{App, AppSpec, Chrome, WindowOptions, WindowSpec};

#[cfg(target_os = "macos")]
use platform::macos::{drag_overlay, menu};
#[cfg(elaris_ui = "macos")]
use platform::macos::{titlebar, window};

fn main() {
    // Matches the desktop entry's id, so Linux desktops show Elaris's name and icon for its windows.
    #[cfg(target_os = "linux")]
    rune::set_app_id("org.elarisbrowser.Elaris");
    let app = configure_app(App::new(app_spec(app::AppState::default, false)));
    #[cfg(target_os = "macos")]
    let app = {
        let mut app = app.keep_running().release_gpu_when_closed();
        drag_overlay::install(&mut app);
        app
    };
    let app_handle = app.handle();
    let app = app.on_ready(move |_| {
        #[cfg(target_os = "macos")]
        menu::install(app_handle.clone());
        start_window_workers(app_handle.clone());
        let wake_handle = app_handle.clone();
        app::termination::install(move || wake_handle.wake());
    });
    app.run();
}

fn app_spec(init: fn() -> app::AppState, mini: bool) -> AppSpec<app::AppState> {
    let (size, min_size) = if mini {
        (ui::theme::MINI_WINDOW_SIZE, ui::theme::MINI_WINDOW_MIN_SIZE)
    } else {
        (ui::theme::WINDOW_SIZE, ui::WINDOW_MIN_SIZE)
    };
    AppSpec {
        // Headless tooling has no native `on_ready`; connect workers there.
        setup: Some(app::setup_headless),
        poll: Some(|state, clock, tx| {
            perf::time(perf::Stage::PollWork, || app::poll(state, clock, tx))
        }),
        on_key: Some(shortcuts::on_key),
        min_size: Some(min_size),
        ..AppSpec::new(app::APP_NAME, size, init, |state, cx| {
            perf::time(perf::Stage::BuildView, || ui::view(state, cx))
        })
    }
}

fn configure_app(app: App<app::AppState>) -> App<app::AppState> {
    let app = app
        .window_options(WindowOptions {
            transparent: true,
            #[cfg(elaris_ui = "linux")]
            borderless: app::LinuxDecorations::current() == app::LinuxDecorations::Elaris,
            ..WindowOptions::default()
        })
        // The tab strip shares the native titlebar band. Rune owns window dragging so
        // AppKit does not steal a drag that started on a tab.
        .chrome(Chrome {
            traffic_lights: Some(ui::TRAFFIC_LIGHTS),
            drag_regions: true,
            ..Chrome::DEFAULT
        });
    #[cfg(elaris_ui = "macos")]
    let app = app.titlebar(ui::theme::native_titlebar(ui::theme::HEADER_HEIGHT));
    #[cfg(elaris_ui = "linux")]
    let app = {
        let app_handle = app.handle();
        app.on_window(move |window| {
            let retained_window = window.clone();
            let register_handle = app_handle.clone();
            register_handle.update(move |state, _| {
                app::register_window(state.window_id, retained_window);
            });
        })
    };
    #[cfg(elaris_ui = "macos")]
    let app = {
        let app_handle = app.handle();
        app.on_window(move |window| {
            let retained_window = window.clone();
            let register_handle = app_handle.clone();
            register_handle.update(move |state, _| {
                app::register_window(state.window_id, retained_window);
            });
            window::install(window);
            titlebar::install(window);
            if let Some(display) = titlebar::display_metadata(window) {
                app_handle.update(move |state, _| state.set_display(display));
            }
        })
    };
    app.modifier_keys()
        .on_frame_drawn(|_| perf::frame_drawn())
        .on_ime(ui::viewport::ime)
        .on_focus(app::on_window_focus)
        .on_close_request(app::AppState::close_requested)
        .on_accessibility_active(|state, active, _| state.browser.set_accessibility(active))
        .on_accessibility_action(|state, request, _| {
            state.browser.accessibility_action(&request);
        })
        .on_close(app::window_closed)
        .on_quit(app::shut_down)
        .on_reopen(app::on_reopen)
        .on_open_url(app::open_external_url)
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
        menu::register_window(state.window_id, menu_handle);
        for session in restored {
            open_browser_window_with_session(session);
        }
    });
}

/// Queues a full browser window in Rune's current event loop turn.
pub(crate) fn open_browser_window() {
    queue_browser_window(app::AppState::new_window(), None);
}

/// Queues a private browser window in Rune's current event loop turn.
pub(crate) fn open_private_browser_window() {
    queue_browser_window(app::AppState::new_private_window(), None);
}

/// Queues a full browser window showing `url`.
pub(crate) fn open_browser_window_with_url(url: String) {
    queue_browser_window(app::AppState::new_window_with_url(&url), None);
}

/// Queues a compact window for a link from another app or a page's popup.
pub(crate) fn open_mini_window(start: app::MiniStart) {
    queue_browser_window(app::AppState::new_mini_window(start), None);
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
    let mini = state.is_mini();
    let app =
        configure_app(App::new(app_spec(app::AppState::new_window, mini)).init(move || state));
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
