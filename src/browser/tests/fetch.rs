// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{TabUpdate, Update};
use elaris::ladybird::MediaAction;

fn opened(commands: &[Command]) -> Vec<TabId> {
    commands
        .iter()
        .filter_map(|command| match command {
            Command::Open(id, _, false) => Some(*id),
            _ => None,
        })
        .collect()
}

#[test]
fn a_page_is_fetched_in_a_hidden_muted_view_that_closes_once_it_reported() {
    let (mut browser, fake) = ready();
    assert!(browser.fetch_page("https://example.com/"));
    assert!(
        !browser.fetch_page("https://example.com/"),
        "already loading"
    );
    let commands = sent(&fake);
    let id = opened(&commands)[0];
    assert!(browser.tab(id).is_none(), "the view is no tab");
    for command in [
        view(id, ViewCommand::MediaAction(MediaAction::ToggleMute)),
        view(id, ViewCommand::Visible(false)),
        view(id, ViewCommand::Load("https://example.com/".into())),
    ] {
        assert!(commands.contains(&command), "{command:?}");
    }

    for update in [
        TabUpdate::Loading(true),
        TabUpdate::Title("Example".into()),
        TabUpdate::Favicon(Some(elaris::ladybird::Favicon {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 4],
        })),
    ] {
        fake.updates.send(Update::Tab(id, update)).unwrap();
    }
    let polled = browser.poll();
    assert_eq!(polled.fetched.len(), 1);
    assert_eq!(polled.fetched[0].title.as_deref(), Some("Example"));
    assert!(polled.fetched[0].favicon.is_some());
    assert!(sent(&fake).contains(&Command::Close(id)));
    assert!(!browser.fetching());
    // Its updates never reached a tab.
    assert!(browser.tabs().iter().all(|tab| tab.title != "Example"));
}

#[test]
fn fetches_are_limited_and_private_windows_make_none() {
    let (mut browser, _fake) = ready();
    assert!(browser.fetch_page("https://one.example/"));
    assert!(browser.fetch_page("https://two.example/"));
    assert!(!browser.fetch_page("https://three.example/"));

    let (mut private, _fake) = ready();
    private.private = true;
    assert!(!private.fetch_page("https://one.example/"));
}
