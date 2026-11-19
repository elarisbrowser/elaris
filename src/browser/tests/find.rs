// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::engine::{FindResult, TabUpdate, Update};

#[test]
fn find_commands_and_results_stay_with_the_named_tab() {
    let (mut browser, fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    let second = browser.open_tab("example.org").unwrap();
    sent(&fake);
    browser.find(first, "river").unwrap();
    browser.find(second, "river").unwrap();
    browser.find_next(first).unwrap();

    assert_eq!(
        sent(&fake),
        [
            view(
                first,
                ViewCommand::Find {
                    query: "river".into(),
                    request_id: 1,
                }
            ),
            view(
                second,
                ViewCommand::Find {
                    query: "river".into(),
                    request_id: 1,
                }
            ),
            view(first, ViewCommand::FindNext { request_id: 2 }),
        ]
    );

    fake.updates
        .send(Update::Tab(
            first,
            TabUpdate::FindResult {
                request_id: 2,
                result: FindResult {
                    current_match_index: 2,
                    total_match_count: Some(3),
                },
            },
        ))
        .unwrap();
    fake.updates
        .send(Update::Tab(
            second,
            TabUpdate::FindResult {
                request_id: 1,
                result: FindResult {
                    current_match_index: 0,
                    total_match_count: Some(1),
                },
            },
        ))
        .unwrap();
    browser.poll();
    assert_eq!(
        browser.tab(first).unwrap().find.result,
        Some(FindResult {
            current_match_index: 2,
            total_match_count: Some(3),
        })
    );
    assert_eq!(
        browser.tab(second).unwrap().find.result,
        Some(FindResult {
            current_match_index: 0,
            total_match_count: Some(1),
        })
    );
}

#[test]
fn internal_pages_reject_find_requests() {
    let (mut browser, fake) = ready();
    let internal = browser.open_tab("elaris://settings").unwrap();
    assert_eq!(
        browser.find(internal, "settings"),
        Err(Error::InternalPage(internal))
    );
    assert_eq!(
        browser.find_next(internal),
        Err(Error::InternalPage(internal))
    );
    assert!(sent(&fake).is_empty());
}

#[test]
fn navigation_and_close_discard_stale_find_results() {
    let (mut browser, fake) = ready();
    let first = browser.open_tab("example.com").unwrap();
    let request_id = {
        browser.find(first, "river").unwrap();
        let ViewCommand::Find { request_id, .. } = sent(&fake)
            .into_iter()
            .find_map(|command| match command {
                Command::View(_, command @ ViewCommand::Find { .. }) => Some(command),
                _ => None,
            })
            .expect("find command is queued")
        else {
            unreachable!("the queued command is a find request")
        };
        request_id
    };

    browser.navigate(first, "next.example").unwrap();
    let stale = Update::Tab(
        first,
        TabUpdate::FindResult {
            request_id,
            result: FindResult {
                current_match_index: 0,
                total_match_count: Some(9),
            },
        },
    );
    fake.updates.send(stale).unwrap();
    assert!(!browser.poll().changed);
    assert_eq!(browser.tab(first).unwrap().find.result, None);
    assert!(sent(&fake).contains(&view(first, ViewCommand::FindEnd)));

    browser.close_tab(first);
    let next = browser.open_tab("other.example").unwrap();
    fake.updates
        .send(Update::Tab(
            first,
            TabUpdate::FindResult {
                request_id: request_id + 1,
                result: FindResult {
                    current_match_index: 0,
                    total_match_count: Some(1),
                },
            },
        ))
        .unwrap();
    assert!(!browser.poll().changed);
    assert!(browser.tab(next).unwrap().find.result.is_none());
}
