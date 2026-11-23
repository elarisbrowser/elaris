// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{Command, Update};
use elaris_core::processes::Process;

fn process(id: u64, kind: &str, site: &str) -> Process {
    Process {
        id,
        pid: 100 + id as i64,
        kind: kind.into(),
        private: false,
        title: None,
        site: site.into(),
        owner: None,
        cpu: 0.5,
        memory: 1024 * 1024,
        spare: false,
        tabs: Vec::new(),
        frames: Vec::new(),
    }
}

#[test]
fn task_manager_lists_engine_processes_and_ends_the_selected_one() {
    let mut state = AppState::new(Some("elaris://processes"));
    let tab = state.browser.active_tab().unwrap().id();
    let (engine, fake) = crate::engine::Engine::fake();
    state.browser.connect(engine);
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::default(),
    );
    runtime.poll = Some(crate::app::poll);
    runtime.frame(0.0);
    let asked = |fake: &crate::engine::FakeEngine| {
        fake.commands
            .try_iter()
            .filter(|command| *command == Command::Processes(tab))
            .count()
    };
    assert_eq!(asked(&fake), 1);
    // The engine has not answered, so the page does not ask again.
    runtime.frame(0.016);
    assert_eq!(asked(&fake), 0);

    fake.updates
        .send(Update::Processes(
            tab,
            Ok(vec![
                process(1, "Browser", ""),
                process(2, "WebContent", "example.com"),
            ]),
        ))
        .unwrap();
    runtime.frame(0.032);
    let overview = runtime.state.task_manager.overview.as_ref().unwrap();
    assert_eq!(overview.browser.len(), 1);
    assert_eq!(overview.sites[0].name, "example.com");

    // The browser's own process cannot be ended.
    runtime.update(|state, _| {
        state.select_process(1);
        state.end_selected_process();
    });
    assert!(
        !fake
            .commands
            .try_iter()
            .any(|command| matches!(command, Command::EndProcess(..)))
    );
    runtime.update(|state, _| {
        state.select_process(2);
        state.end_selected_process();
    });
    assert!(
        fake.commands
            .try_iter()
            .any(|command| command == Command::EndProcess(tab, 2))
    );
    assert_eq!(runtime.state.task_manager.selected, None);
}

#[test]
fn task_manager_reports_a_missing_engine() {
    let mut runtime = rune::Runtime::new(
        AppState::new(Some("elaris://processes")),
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::default(),
    );
    runtime.poll = Some(crate::app::poll);
    runtime.frame(0.0);
    assert!(runtime.state.task_manager.load_error.is_some());
}
