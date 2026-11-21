// SPDX-License-Identifier: GPL-3.0-only

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use elaris::ladybird::{
    BrowsingData, Event, Ladybird, MouseEvent, MouseKind, PermissionPrompt, WebView, WheelPhase,
    button,
};

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
            Event::PermissionPrompt(_)
                | Event::PermissionSaved { .. }
                | Event::Title(_)
                | Event::ExternalUrl(_)
                | Event::BrowsingDataCleared { .. }
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

    let external = page.replace("permissions.html", "external.html");
    view.resize(800, 600, 1.0)?;
    view.set_focus(true)?;
    view.load(&format!("{external}?missing"))?;
    wait_for_title(&mut browser, &events, "External link test")?;
    click(&mut view)?;
    let notice = wait_for_external(&mut browser, &events, "a missing handler notice")?;
    assert!(notice.unavailable && notice.url.contains("elaris-test-missing-handler"));
    view.reply_to_external_url(notice.request_id, false)?;

    if cfg!(target_os = "macos") {
        view.load(&format!("{external}?known"))?;
        wait_for_title(&mut browser, &events, "External link test")?;
        click(&mut view)?;
        let request = wait_for_external(&mut browser, &events, "an external link prompt")?;
        assert!(!request.unavailable && request.origin == origin);
        assert!(!request.application.is_empty());
        view.reply_to_external_url(request.request_id, false)?;

        click(&mut view)?;
        let stale = wait_for_external(&mut browser, &events, "a second external link prompt")?;
        view.load(&page)?;
        wait_for(
            &mut browser,
            &events,
            "navigation to dismiss the link prompt",
            |event| matches!(event, Event::ExternalUrl(None)).then_some(()),
        )?;
        view.reply_to_external_url(stale.request_id, true)?;
        wait_for_title(&mut browser, &events, "Permission test")?;
    }
    println!("Passed external link notice, prompt, cancel and navigation dismissal");

    let storage = page.replace("permissions.html", "storage.html");
    view.load(&storage)?;
    wait_for_title(&mut browser, &events, "Fresh")?;
    view.reload()?;
    wait_for_title(&mut browser, &events, "Stored")?;
    let site_data = BrowsingData {
        site_data: true,
        ..BrowsingData::default()
    };
    browser.clear_browsing_data(None, site_data, 7)?;
    let cleared = wait_for(
        &mut browser,
        &events,
        "site data to clear",
        |event| match event {
            Event::BrowsingDataCleared {
                request: 7,
                cleared,
            } => Some(cleared),
            _ => None,
        },
    )?;
    assert!(cleared);
    view.reload()?;
    wait_for_title(&mut browser, &events, "Fresh")?;
    println!("Passed clearing site data");

    browser.set_autoplay(2, &[(origin.as_str(), 0)])?;
    assert!(browser.set_autoplay(3, &[]).is_err());
    browser.set_autoplay(1, &[])?;
    println!("Passed autoplay settings");
    Ok(())
}

fn click(view: &mut WebView) -> TestResult<()> {
    for (kind, buttons) in [(MouseKind::Down, button::PRIMARY), (MouseKind::Up, 0)] {
        view.mouse_event(MouseEvent {
            kind,
            x: 100,
            y: 100,
            button: button::PRIMARY,
            buttons,
            modifiers: 0,
            wheel: (0.0, 0.0),
            precise: false,
            phase: WheelPhase::None,
            clicks: 1,
        })?;
    }
    Ok(())
}

fn wait_for_external(
    browser: &mut Ladybird,
    events: &Events,
    description: &str,
) -> TestResult<elaris::ladybird::ExternalUrl> {
    wait_for(browser, events, description, |event| match event {
        Event::ExternalUrl(Some(request)) => Some(request),
        _ => None,
    })
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
