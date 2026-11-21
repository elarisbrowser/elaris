// SPDX-License-Identifier: GPL-3.0-only

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use elaris::ladybird::{Event, Ladybird, PermissionPrompt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let resources = PathBuf::from(args.next().ok_or("missing Ladybird resources")?);
    let profile = PathBuf::from(args.next().ok_or("missing temporary profile directory")?);
    let page = args.next().ok_or("missing loopback permission page URL")?;
    let (origin, _) = page
        .strip_prefix("http://127.0.0.1:")
        .and_then(|url| url.split_once('/'))
        .ok_or("the permission page must be served on 127.0.0.1")?;
    let origin = format!("http://127.0.0.1:{origin}");
    let location = format!("{page}?location");
    let away = format!("{page}?away");

    let events = Rc::new(RefCell::new(VecDeque::new()));
    let sink = Rc::clone(&events);
    let mut browser = Ladybird::new(&resources, &profile, move |_, event| {
        if matches!(
            event,
            Event::PermissionPrompt(_) | Event::PermissionSaved { .. } | Event::Title(_)
        ) {
            sink.borrow_mut().push_back(event);
        }
    })?;
    let mut view = browser.create_view(1, 60.0)?;
    browser.set_permissions(&[])?;

    view.load(&location)?;
    let request = wait_for_prompt(&mut browser, &events, &origin)?;
    view.reply_to_permission(request.request_id, Some(true))?;
    wait_for_choice(&mut browser, &events, &origin, true)?;
    wait_for_title(&mut browser, &events, "Location granted")?;
    view.reload()?;
    wait_for_title(&mut browser, &events, "Location granted")?;

    browser.set_permission(&origin, "location", None)?;
    view.reload()?;
    let request = wait_for_prompt(&mut browser, &events, &origin)?;
    view.reply_to_permission(request.request_id, Some(false))?;
    wait_for_choice(&mut browser, &events, &origin, false)?;
    wait_for_title(&mut browser, &events, "Location error 1")?;
    view.reload()?;
    wait_for_title(&mut browser, &events, "Location error 1")?;

    browser.set_permission(&origin, "location", None)?;
    view.reload()?;
    let stale = wait_for_prompt(&mut browser, &events, &origin)?;
    view.load(&away)?;
    wait_for(
        &mut browser,
        &events,
        "navigation to dismiss the prompt",
        |event| matches!(event, Event::PermissionPrompt(None)).then_some(()),
    )?;
    wait_for_title(&mut browser, &events, "Navigated away")?;
    view.reply_to_permission(stale.request_id, Some(true))?;
    view.load(&location)?;
    let current = wait_for_prompt(&mut browser, &events, &origin)?;
    assert_ne!(stale.request_id, current.request_id);
    view.reply_to_permission(stale.request_id, Some(true))?;
    view.reply_to_permission(current.request_id, Some(false))?;
    wait_for_choice(&mut browser, &events, &origin, false)?;
    wait_for_title(&mut browser, &events, "Location error 1")?;

    browser.set_permission(&origin, "location", None)?;
    view.load(&format!("{page}?frame"))?;
    let frame_request = wait_for_prompt(&mut browser, &events, &origin)?;
    let mut removed = false;
    let mut dismissed = false;
    wait_for(
        &mut browser,
        &events,
        "frame removal and prompt dismissal",
        |event| {
            match event {
                Event::Title(title) if title == "Frame removed" => removed = true,
                Event::PermissionPrompt(None) => dismissed = true,
                _ => {}
            }
            (removed && dismissed).then_some(())
        },
    )?;
    view.reply_to_permission(frame_request.request_id, Some(true))?;

    browser.set_permission_default("location", true)?;
    view.load(&location)?;
    wait_for_title(&mut browser, &events, "Location error 1")?;
    browser.set_permission(&origin, "location", Some(true))?;
    view.reload()?;
    wait_for_title(&mut browser, &events, "Location granted")?;

    println!(
        "Passed geolocation allow, block, reset, navigation, frame removal, stale replies and default exceptions"
    );
    if std::env::var_os("ELARIS_TEST_MICROPHONE_DENIED").is_some_and(|value| value == "1") {
        let microphone = format!("{page}?microphone");
        browser.set_permission(&origin, "microphone", Some(false))?;
        view.load(&microphone)?;
        wait_for_title(&mut browser, &events, "Microphone error NotAllowedError")?;

        browser.set_permission(&origin, "microphone", None)?;
        view.reload()?;
        let mut notice = None;
        let mut rejected = false;
        let request_id = wait_for(&mut browser, &events, "an OS microphone denial", |event| {
            match event {
                Event::PermissionPrompt(Some(prompt)) => {
                    assert_eq!(prompt.origin, origin);
                    assert_eq!(prompt.key, "microphone");
                    assert!(prompt.os_denied);
                    notice = Some(prompt.request_id);
                }
                Event::Title(title) if title == "Microphone error NotAllowedError" => {
                    rejected = true
                }
                Event::PermissionSaved { .. } => {
                    panic!("An OS denial must not record a site decision")
                }
                _ => {}
            }
            if rejected { notice } else { None }
        })?;
        view.reply_to_permission(request_id, None)?;
        println!("Distinguished browser microphone block from OS microphone denial");
    }
    Ok(())
}

type Events = RefCell<VecDeque<Event>>;
type TestResult<T> = Result<T, Box<dyn std::error::Error>>;

fn wait_for<T>(
    browser: &mut Ladybird,
    events: &Events,
    description: &str,
    mut inspect: impl FnMut(Event) -> Option<T>,
) -> TestResult<T> {
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        while let Some(event) = events.borrow_mut().pop_front() {
            if let Some(value) = inspect(event) {
                return Ok(value);
            }
        }
        browser.wait(Some(Duration::from_millis(20)))?;
    }
    Err(format!("Timed out waiting for {description}").into())
}

fn wait_for_prompt(
    browser: &mut Ladybird,
    events: &Events,
    origin: &str,
) -> TestResult<PermissionPrompt> {
    wait_for(
        browser,
        events,
        "a geolocation prompt",
        |event| match event {
            Event::PermissionPrompt(Some(prompt)) => {
                assert_eq!(prompt.origin, origin);
                assert_eq!(prompt.key, "location");
                assert!(!prompt.os_denied);
                Some(prompt)
            }
            _ => None,
        },
    )
}

fn wait_for_choice(
    browser: &mut Ladybird,
    events: &Events,
    origin: &str,
    allowed: bool,
) -> TestResult<()> {
    wait_for(
        browser,
        events,
        "the saved geolocation choice",
        |event| match event {
            Event::PermissionSaved {
                origin: site,
                key,
                allowed: choice,
            } => {
                assert_eq!(site, origin);
                assert_eq!(key, "location");
                assert_eq!(choice, allowed);
                Some(())
            }
            _ => None,
        },
    )
}

fn wait_for_title(browser: &mut Ladybird, events: &Events, title: &str) -> TestResult<()> {
    wait_for(browser, events, title, |event| match event {
        Event::Title(actual) if actual == title => Some(()),
        Event::PermissionPrompt(Some(prompt)) => {
            panic!("Unexpected prompt {prompt:?} while waiting for {title}")
        }
        _ => None,
    })
}
