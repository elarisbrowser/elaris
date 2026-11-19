// SPDX-License-Identifier: GPL-3.0-only

use super::{
    Browser, Command, Engine, Error, InternalPage, InternalRoute, Surface, Tab, TabId, ViewCommand,
};
use crate::engine::FakeEngine;

mod downloads;
mod engine;
mod find;
mod navigation;
mod tabs;
mod views;

fn connected() -> (Browser, FakeEngine) {
    let mut browser = Browser::default();
    let (engine, fake) = Engine::fake();
    browser.connect(engine);
    (browser, fake)
}

fn sent(fake: &FakeEngine) -> Vec<Command> {
    fake.commands.try_iter().collect()
}

/// What the view does as it builds and lays out.
fn lay_out(browser: &mut Browser, size: [f32; 2], scale: f32, dark: bool) {
    browser.note_scale(scale);
    browser.note_dark(dark);
    browser.set_viewport(size);
}

/// A browser laid out at 100x50 points, scale 1, with its commands so far read.
fn ready() -> (Browser, FakeEngine) {
    let (mut browser, fake) = connected();
    lay_out(&mut browser, [100.0, 50.0], 1.0, false);
    browser.poll();
    sent(&fake);
    (browser, fake)
}

fn view(id: TabId, command: ViewCommand) -> Command {
    Command::View(id, command)
}

fn resize(width: i32, height: i32, scale: f64) -> ViewCommand {
    ViewCommand::Resize {
        width,
        height,
        scale,
    }
}

fn surface(browser: &Browser, id: TabId) -> Surface {
    browser.tab(id).unwrap().surface.clone()
}
