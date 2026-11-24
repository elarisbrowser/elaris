// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::omnibox;
use elaris_core::address::SearchEngine;
use elaris_core::appearance::Appearance;
use elaris_core::settings::{ContentBlockList, DownloadDirectory};
use rune::Mods;
use std::collections::BTreeSet;

#[test]
fn the_settings_shortcut_reuses_its_existing_native_tab() {
    let mut state = AppState::new(Some("example.com"));
    let website = state.browser.active_tab().unwrap().id();
    let mut tx = Tx::default();
    let event = KeyEvent {
        key: Key::Command(','),
        mods: Mods {
            cmd: true,
            ..Mods::default()
        },
    };
    assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
    let settings = state.browser.active_tab().unwrap().id();
    assert!(state.browser.active_tab().unwrap().is_internal());
    assert_eq!(state.browser.tabs().len(), 2);

    state.activate(website);
    assert!(crate::shortcuts::on_key(&mut state, &event, &mut tx));
    assert_eq!(state.browser.tabs().len(), 2);
    assert_eq!(state.browser.active_tab().unwrap().id(), settings);
}
#[test]
fn settings_navigation_shows_one_category_in_narrow_and_wide_windows() {
    // The narrow window folds the navigation under the title; the wide one keeps it beside.
    for size in [(700.0, 640.0), (1280.0, 800.0)] {
        let state = AppState::new(Some("elaris://settings"));
        let mut runtime =
            rune::Runtime::new(state, crate::ui::view, size, rune::Env::new(false, 1.0));
        runtime.frame(0.0);

        let find = |runtime: &rune::Runtime<AppState>, key: &str| {
            runtime
                .elements()
                .into_iter()
                .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
        };
        assert!(find(&runtime, "settings-look").is_some());
        assert!(find(&runtime, "settings-privacy").is_none());

        let [x, y, width, height] = find(&runtime, "settings-privacy-nav")
            .expect("privacy navigation")
            .rect;
        runtime.pointer_down(x + width / 2.0, y + height / 2.0);
        runtime.pointer_up(x + width / 2.0, y + height / 2.0);
        runtime.frame(0.0);

        assert_eq!(runtime.state.page_category, Some("settings-privacy"));
        assert!(find(&runtime, "settings-look").is_none(), "{size:?}");
        let heading = find(&runtime, "settings-privacy").expect("privacy category");
        let scroll = find(&runtime, "settings-scroll").expect("scroll region");
        assert!(
            (0.0..=32.0).contains(&(heading.rect[1] - scroll.rect[1])),
            "heading {:?}, scroll region {:?}",
            heading.rect,
            scroll.rect,
        );
    }
}

#[test]
fn settings_navigation_highlights_glide_to_the_hovered_and_chosen_items() {
    let state = AppState::new(Some("elaris://settings"));
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    let rect = |runtime: &rune::Runtime<AppState>, key: &str| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
            .map(|element| element.rect)
    };
    let centre = |[x, y, width, height]: [f32; 4]| (x + width / 2.0, y + height / 2.0);
    let first = rect(&runtime, "settings-look-nav").unwrap();
    let privacy = rect(&runtime, "settings-privacy-nav").unwrap();
    let setup = rect(&runtime, "settings-system-nav").unwrap();
    assert!(rect(&runtime, "page-nav-hover").is_none());
    assert_eq!(rect(&runtime, "page-nav-selection").unwrap()[1], first[1]);

    // The hover highlight appears under the pointer, then glides rather than jumping.
    let (x, y) = centre(privacy);
    runtime.pointer_move(x, y);
    runtime.frame(0.0);
    assert_eq!(rect(&runtime, "page-nav-hover").unwrap()[1], privacy[1]);
    let (x, y) = centre(setup);
    runtime.pointer_move(x, y);
    runtime.frame(0.016);
    let gliding = rect(&runtime, "page-nav-hover").unwrap()[1];
    assert!(gliding > privacy[1] && gliding < setup[1], "{gliding}");
    for _ in 0..60 {
        runtime.frame(0.016);
    }
    assert_eq!(rect(&runtime, "page-nav-hover").unwrap()[1], setup[1]);

    // Choosing the hovered item slides the selection there; the hover highlight steps aside.
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(0.016);
    let sliding = rect(&runtime, "page-nav-selection").unwrap()[1];
    assert!(sliding > first[1] && sliding < setup[1], "{sliding}");
    for _ in 0..60 {
        runtime.frame(0.016);
    }
    assert_eq!(rect(&runtime, "page-nav-selection").unwrap()[1], setup[1]);

    // Pausing on the selected item keeps the hover highlight, so the next move still glides.
    let (x, y) = centre(privacy);
    runtime.pointer_move(x, y);
    runtime.frame(0.016);
    let gliding = rect(&runtime, "page-nav-hover").unwrap()[1];
    assert!(gliding > privacy[1] && gliding < setup[1], "{gliding}");

    // Leaving the list clears it.
    runtime.pointer_move(1.0, 790.0);
    for _ in 0..60 {
        runtime.frame(0.016);
    }
    assert!(rect(&runtime, "page-nav-hover").is_none());
}

#[test]
fn settings_updates_drive_search_download_and_startup_preferences() {
    let mut state = AppState::new(Some("example.com"));
    state.set_appearance(Appearance::Dark);
    state.set_search_engine(SearchEngine::Brave);
    state.set_startup(StartupMode::BlankWindow);
    let download_path = PathBuf::from("/tmp/elaris-test-downloads");
    state.set_download_directory(DownloadDirectory::Custom(download_path.clone()));

    assert_eq!(state.settings.appearance, Appearance::Dark);
    assert_eq!(state.settings.search_engine, SearchEngine::Brave);
    assert_eq!(state.settings.startup, StartupMode::BlankWindow);
    assert_eq!(state.download_directory(), download_path);

    state.open_omnibox(omnibox::Mode::Current);
    state
        .omnibox
        .input
        .replace_text("browser settings".to_owned());
    state.submit_omnibox();
    assert_eq!(
        active_url(&state),
        "https://search.brave.com/search?q=browser%20settings"
    );
}

#[test]
fn content_blocking_settings_keep_the_enabled_lists() {
    let mut state = AppState::new(Some("example.com"));
    assert!(!state.settings.content_blocking.enabled);

    state.set_content_blocking(true);
    state.set_content_blocking_list(ContentBlockList::CookieNotices, true);
    state.set_content_blocking_list(ContentBlockList::EasyPrivacy, false);

    assert!(state.settings.content_blocking.enabled);
    assert_eq!(
        state.settings.content_blocking.lists,
        BTreeSet::from([ContentBlockList::EasyList, ContentBlockList::CookieNotices])
    );
}

#[test]
fn internal_pages_open_a_native_context_menu_with_page_actions() {
    let state = AppState::new(Some("elaris://settings"));
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::new(false, 1.0),
    );
    runtime.frame(0.0);
    let labels = |runtime: &rune::Runtime<AppState>| {
        runtime
            .elements()
            .into_iter()
            .filter(|element| {
                element.role == "menuitem"
                    && !element.exiting
                    && element
                        .key
                        .as_deref()
                        .is_some_and(|key| key.starts_with("page-context-menu"))
            })
            .filter_map(|element| element.label)
            .collect::<Vec<_>>()
    };

    runtime.secondary_down(1200.0, 700.0);
    runtime.secondary_up(1200.0, 700.0);
    runtime.frame(0.0);
    assert!(runtime.state.page_menu.open());
    assert_eq!(
        labels(&runtime),
        [
            "Edit config.toml",
            "Reload config.toml",
            "Copy Page Address",
            "History",
            "Bookmarks",
            "Task Manager",
        ]
    );

    runtime.update(crate::app::open_history);
    for _ in 0..30 {
        runtime.frame(0.016);
    }
    assert!(!runtime.state.page_menu.open());
    runtime.secondary_down(1200.0, 700.0);
    runtime.secondary_up(1200.0, 700.0);
    runtime.frame(0.0);
    let history = labels(&runtime);
    assert_eq!(history[0], "Refresh");
    assert!(history.contains(&"Settings".to_owned()));
    assert!(!history.contains(&"History".to_owned()));
    assert!(history.contains(&"Task Manager".to_owned()));
}

/// The menu highlight lives inside the panel's rows, not in a window overlay placed from last
/// frame's layout, which a native page such as Settings left hidden while a row was hovered.
#[test]
fn menu_highlight_stays_inside_the_panel_over_settings() {
    let state = AppState::new(Some("elaris://settings"));
    let tab = state.browser.active_tab().unwrap().id();
    let mut runtime = rune::Runtime::new(
        state,
        crate::ui::view,
        (1280.0, 800.0),
        rune::Env::new(false, 1.0),
    );
    runtime.poll = Some(crate::app::poll);
    runtime.frame(0.0);
    runtime
        .state
        .open_context_menu(Some(tab), rune::PointerPress { x: 120.0, y: 160.0 });
    let settle = |runtime: &mut rune::Runtime<AppState>| {
        for _ in 0..35 {
            runtime.frame(1.0 / 60.0);
        }
    };
    settle(&mut runtime);
    let element = |runtime: &rune::Runtime<AppState>, key: &str| {
        runtime
            .elements()
            .into_iter()
            .find(|element| element.key.as_deref() == Some(key) && !element.exiting)
            .unwrap_or_else(|| panic!("missing {key}"))
    };
    let row = element(&runtime, "sidebar-context-menu-item1").rect;
    runtime.pointer_move(row[0] + row[2] / 2.0, row[1] + row[3] / 2.0);
    // Long enough for any poll that reads focus to have taken the highlight away.
    for _ in 0..4 {
        settle(&mut runtime);
    }
    let highlight = element(&runtime, "sidebar-context-menu-highlight");
    assert_eq!(highlight.rect[1], row[1]);
    assert!(
        highlight.path.contains("sidebar-context-menu-menu"),
        "the highlight must be drawn inside the menu panel: {}",
        highlight.path
    );
}

#[test]
fn private_site_controls_use_the_view_without_changing_saved_site_settings() {
    use crate::engine::{Command, Engine, PrivatePermissionChange, TabUpdate};
    use crate::site_controls::SiteControlPresentation;
    use elaris_core::permissions::{AutoplayPolicy, PermissionKind, PermissionState};

    let mut state = AppState::new(Some("https://example.com"));
    state.browser.private = true;
    let origin = "https://example.com";
    let tab = state.browser.active_tab().unwrap().id();
    state
        .browser
        .tab_mut(tab)
        .unwrap()
        .update(TabUpdate::NavigationCommitted(origin.into()));
    let kind = PermissionKind::Microphone;
    state.permission_store.set(origin, kind, PermissionState::Block);
    state.permission_store.set_autoplay(origin, Some(AutoplayPolicy::Block));
    let saved = state.permission_store.clone();
    let (engine, fake) = Engine::fake();
    state.browser.connect(engine);
    fake.commands.try_iter().for_each(drop);

    state.refresh_private_site_permissions();
    let before = SiteControlPresentation::from_state(&state);
    assert!(before.private);
    assert!(!before.permissions_ready);
    assert_eq!(before.autoplay, None);

    state.set_site_permission(kind, PermissionState::Allow);
    state.set_site_permission(PermissionKind::Camera, PermissionState::Allow);
    state.set_site_autoplay(Some(AutoplayPolicy::Allow));
    state.reset_site_permissions();
    assert_eq!(state.permission_store, saved);
    assert_eq!(
        fake.commands.try_iter().collect::<Vec<_>>(),
        [
            Command::PrivateSitePermissions(tab, origin.into(), PrivatePermissionChange::Read),
            Command::PrivateSitePermissions(
                tab,
                origin.into(),
                PrivatePermissionChange::Set(kind, PermissionState::Allow),
            ),
            Command::PrivateSitePermissions(tab, origin.into(), PrivatePermissionChange::Reset),
        ]
    );

    let mut permissions = [PermissionState::Ask; PermissionKind::ALL.len()];
    permissions[kind.index()] = PermissionState::Allow;
    let view = state.browser.tab_mut(tab).unwrap();
    assert!(!view.update(TabUpdate::PrivateSitePermissions(
        "https://another.example".into(),
        permissions,
    )));
    assert!(view.update(TabUpdate::PrivateSitePermissions(origin.into(), permissions)));
    let after = SiteControlPresentation::from_state(&state);
    assert!(after.permissions_ready);
    assert_eq!(after.permissions[kind.index()].1, PermissionState::Allow);

    state.browser.private = false;
    let regular = SiteControlPresentation::from_state(&state);
    assert_eq!(regular.permissions[kind.index()].1, PermissionState::Block);
    assert_eq!(regular.autoplay, Some(AutoplayPolicy::Block));
}
