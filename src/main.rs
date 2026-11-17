// SPDX-License-Identifier: GPL-3.0-only

mod address;
mod app;
mod engine;
mod input;
mod page;
mod perf;
mod ui;

use rune::{App, AppSpec};

fn main() {
    let spec = AppSpec {
        poll: Some(app::poll),
        ..AppSpec::new("Elaris", (960.0, 640.0), app::AppState::default, ui::view)
    };
    App::new(spec)
        .on_focus(app::on_window_focus)
        .on_close(app::shut_down)
        .on_quit(app::shut_down)
        .run();
}
