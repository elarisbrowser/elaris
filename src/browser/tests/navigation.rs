// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{TabUpdate, Update};

#[test]
fn internal_tabs_never_open_ladybird_views_and_close_like_other_tabs() {
    let (mut browser, fake) = connected();
    let settings = browser.open_tab("elaris://settings").unwrap();
    let tab = browser
        .tab(settings)
        .expect("native settings tab is present");
    assert!(tab.is_internal());
    assert_eq!(tab.title, "Settings");
    assert_eq!(tab.engine.pending_load, None);
    browser.poll();
    assert!(
        sent(&fake).is_empty(),
        "native pages never create engine views"
    );

    let website = browser.open_tab("example.com").unwrap();
    assert!(matches!(sent(&fake).as_slice(), [Command::Open(id, _)] if *id == website));
    browser.close_tab(settings);
    assert!(browser.tab(settings).is_none());
    assert_eq!(browser.active_tab().map(Tab::id), Some(website));
    assert!(
        sent(&fake).is_empty(),
        "closing a native page sends no engine command"
    );
}

#[test]
fn navigation_and_history_go_to_the_tab_named() {
    let (mut browser, fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    browser.poll();
    sent(&fake);

    browser.navigate(second, "ladybird.org").unwrap();
    browser.reload(first);
    browser.go_back(second);
    browser.go_forward(first);
    assert_eq!(
        sent(&fake),
        [
            view(second, ViewCommand::Load("https://ladybird.org".into())),
            view(first, ViewCommand::Reload),
            view(second, ViewCommand::Traverse(-1)),
            view(first, ViewCommand::Traverse(1)),
        ]
    );
    assert_eq!(browser.tab(second).unwrap().url, "https://ladybird.org");
    assert_eq!(browser.tab(first).unwrap().url, "https://example.com");
}

#[test]
fn navigation_between_websites_and_native_pages_opens_and_closes_views() {
    let (mut browser, fake) = ready();
    let tab = browser.open_tab("example.com").unwrap();
    browser.poll();
    sent(&fake);

    browser
        .navigate_internal(tab, InternalRoute::Page(InternalPage::Settings))
        .unwrap();
    assert!(browser.tab(tab).unwrap().is_internal());
    assert_eq!(sent(&fake), [Command::Close(tab)]);

    browser.navigate(tab, "example.org").unwrap();
    assert!(!browser.tab(tab).unwrap().is_internal());
    assert_eq!(sent(&fake), [Command::Open(tab, surface(&browser, tab))]);
    browser.poll();
    assert!(sent(&fake).contains(&view(tab, ViewCommand::Load("https://example.org".into()))));
}

#[test]
fn only_committed_navigation_updates_are_reported_for_history() {
    let (mut browser, fake) = connected();
    let tab = browser.open_tab("example.com").unwrap();
    fake.updates
        .send(Update::Tab(
            tab,
            TabUpdate::Navigation("https://provisional.example/".into()),
        ))
        .unwrap();
    assert!(browser.poll().committed.is_empty());

    fake.updates
        .send(Update::Tab(
            tab,
            TabUpdate::NavigationCommitted("https://committed.example/".into()),
        ))
        .unwrap();
    let polled = browser.poll();
    assert_eq!(
        polled.committed,
        [(tab, "https://committed.example/".into(), String::new())]
    );
    assert_eq!(
        browser.tab(tab).unwrap().committed_url,
        "https://committed.example/"
    );
}

#[test]
fn provisional_titles_do_not_update_the_previous_history_entry() {
    let (mut browser, fake) = connected();
    let tab = browser.open_tab("example.com").unwrap();
    fake.updates
        .send(Update::Tab(
            tab,
            TabUpdate::NavigationCommitted("https://example.com/".into()),
        ))
        .unwrap();
    browser.poll();

    fake.updates
        .send(Update::Tab(
            tab,
            TabUpdate::Navigation("https://iana.org/domains/reserved".into()),
        ))
        .unwrap();
    fake.updates
        .send(Update::Tab(
            tab,
            TabUpdate::Title("IANA-managed Reserved Domains".into()),
        ))
        .unwrap();
    assert!(browser.poll().titles.is_empty());

    fake.updates
        .send(Update::Tab(
            tab,
            TabUpdate::NavigationCommitted("https://iana.org/domains/reserved".into()),
        ))
        .unwrap();
    browser.poll();
    fake.updates
        .send(Update::Tab(
            tab,
            TabUpdate::Title("IANA-managed Reserved Domains".into()),
        ))
        .unwrap();
    assert_eq!(
        browser.poll().titles,
        [(
            "https://iana.org/domains/reserved".into(),
            "IANA-managed Reserved Domains".into()
        )]
    );
}

#[test]
fn restored_internal_tabs_stay_native_and_do_not_record_history() {
    let (mut browser, fake) = connected();
    let settings = browser
        .restore_tab("elaris://settings", "Old title", true)
        .unwrap();
    let website = browser
        .restore_tab("https://example.com/", "Example", true)
        .unwrap();
    browser.activate_tab(settings);
    assert_eq!(browser.tab(settings).unwrap().title, "Settings");
    assert_eq!(
        browser.tab(settings).unwrap().internal,
        Some(InternalRoute::Page(InternalPage::Settings))
    );

    fake.updates
        .send(Update::Tab(
            settings,
            crate::engine::TabUpdate::NavigationCommitted("elaris://settings".into()),
        ))
        .unwrap();
    assert!(browser.poll().committed.is_empty());
    assert_eq!(
        sent(&fake),
        [
            Command::Open(website, surface(&browser, website)),
            view(website, ViewCommand::Visible(false)),
        ]
    );
}

#[test]
fn searches_ordinary_text_and_rejects_unsupported_schemes() {
    let (mut browser, fake) = ready();
    let tab = browser.open_tab("example.com").unwrap();
    assert!(matches!(
        browser.open_tab("ftp://example.com"),
        Err(Error::Address(_))
    ));
    browser.navigate(tab, "two words").unwrap();
    assert_eq!(
        browser.tab(tab).unwrap().url,
        "https://duckduckgo.com/?q=two%20words"
    );
    assert!(matches!(
        browser.navigate(tab, "ftp://example.com"),
        Err(Error::Address(_))
    ));
    browser.close_tab(tab);
    assert_eq!(
        browser.navigate(tab, "example.org"),
        Err(Error::NoSuchTab(tab))
    );
    assert_eq!(browser.tabs().len(), 0);
    // Only the one tab opened and closed; nothing else reached the engine.
    assert_eq!(
        sent(&fake).last(),
        Some(&Command::Close(tab)),
        "a closed tab takes no commands"
    );
}

#[test]
fn website_navigation_cannot_open_privileged_internal_pages() {
    let (mut browser, fake) = ready();
    let website = browser.open_tab("example.com").unwrap();
    browser.poll();
    sent(&fake);

    assert!(browser.navigate(website, "elaris://settings").is_err());
    fake.updates
        .send(Update::Tab(
            website,
            TabUpdate::NavigationCommitted("elaris://settings".into()),
        ))
        .unwrap();
    let result = browser.poll();
    assert!(result.committed.is_empty());
    assert_eq!(browser.tab(website).unwrap().url, "https://example.com");
    assert!(!browser.tab(website).unwrap().is_internal());
}
