mod app;
mod ui;

use rune::AppSpec;

fn main() {
    rune::run(AppSpec::new(
        "Elaris",
        (960.0, 640.0),
        app::AppState::default,
        ui::view,
    ));
}
