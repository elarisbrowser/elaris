// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use super::*;

#[test]
#[ignore = "requires a built Ladybird bridge and helper processes"]
fn hidden_frames_keep_two_recent_views_and_restore_idle_tabs() {
    let profile =
        std::env::temp_dir().join(format!("elaris-frame-eviction-{}", std::process::id()));
    let (mut runtime, _) = Runtime::start(profile.clone(), None, 60.0.into()).unwrap();
    runtime.hidden_order = Some(Vec::new());
    let url = format!("file://{}/benchmarks/idle.html", env!("CARGO_MANIFEST_DIR"));
    let tabs: Vec<_> = (1..=5)
        .map(|id| (TabId::from_raw(id), Surface::default()))
        .collect();
    let size = (400, 300);
    let mut previews = HashMap::new();

    for (id, surface) in &tabs {
        surface.request_preview_invalidation();
        for command in [
            Command::Open(*id, surface.clone(), false),
            Command::View(
                *id,
                ViewCommand::Resize {
                    width: size.0 as i32,
                    height: size.1 as i32,
                    scale: 1.0,
                },
            ),
            Command::View(*id, ViewCommand::Load(url.clone())),
        ] {
            assert!(runtime.apply(command).is_none());
        }
    }
    let until = Instant::now() + Duration::from_secs(10);
    while previews.len() < tabs.len() && Instant::now() < until {
        runtime
            .ladybird
            .wait(Some(Duration::from_millis(100)))
            .unwrap();
        for update in runtime.handle_events() {
            if let Update::Thumbnail(id, thumbnail) = update {
                previews.insert(id, thumbnail);
            }
        }
    }
    assert_eq!(previews.len(), tabs.len());
    for (id, _) in &tabs[..3] {
        assert!(
            runtime
                .apply(Command::View(*id, ViewCommand::Visible(false)))
                .is_none()
        );
    }
    runtime.trim_hidden_frames();

    let (evicted, surface) = &tabs[0];
    assert_eq!(surface.pixels.size(), (160, 120));
    assert!(previews[evicted].get(surface).is_some());
    for (_, surface) in &tabs[1..] {
        assert_eq!(surface.pixels.size(), size);
    }
    assert!(
        runtime
            .apply(Command::View(*evicted, ViewCommand::Visible(true)))
            .is_none()
    );
    assert_eq!(surface.pixels.size(), size);
    runtime.close();
    std::fs::remove_dir_all(profile).unwrap();
}

#[test]
#[ignore = "requires a built Ladybird bridge and helper processes"]
fn visible_views_sample_their_own_ladybird_frames() {
    let profile = std::env::temp_dir().join(format!("elaris-thumbnails-{}", std::process::id()));
    let (mut runtime, _) = Runtime::start(profile.clone(), None, 60.0.into()).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("bridge/tests/smoke.html");
    let url = format!("file://{}", path.display());
    let left = TabId::from_raw(1);
    let right = TabId::from_raw(2);
    let hidden = TabId::from_raw(3);
    let left_surface = Surface::default();
    let right_surface = Surface::default();
    for (id, width, surface) in [
        (left, 640, left_surface.clone()),
        (right, 320, right_surface.clone()),
        (hidden, 480, Surface::default()),
    ] {
        assert!(
            runtime
                .apply(Command::Open(id, surface.clone(), false))
                .is_none()
        );
        if id == hidden {
            assert!(
                runtime
                    .apply(Command::View(id, ViewCommand::Visible(false)))
                    .is_none()
            );
        }
        assert!(
            runtime
                .apply(Command::View(
                    id,
                    ViewCommand::Resize {
                        width,
                        height: 480,
                        scale: 1.0,
                    }
                ))
                .is_none()
        );
        surface.request_preview_invalidation();
        assert!(
            runtime
                .apply(Command::View(id, ViewCommand::Load(url.clone())))
                .is_none()
        );
    }
    let until = Instant::now() + Duration::from_secs(10);
    let mut samples = HashMap::new();
    while samples.len() < 2 && Instant::now() < until {
        runtime
            .ladybird
            .wait(Some(Duration::from_millis(100)))
            .unwrap();
        for update in runtime.handle_events() {
            if let Update::Thumbnail(id, thumbnail) = update {
                samples.insert(id, thumbnail);
            }
        }
    }
    let dimensions = |id, surface| samples.get(&id)?.get(surface).map(|pixels| pixels.size());
    let left_size = dimensions(left, &left_surface);
    let right_size = dimensions(right, &right_surface);
    assert!(dimensions(left, &right_surface).is_none());
    assert!(dimensions(right, &left_surface).is_none());

    left_surface.request_preview_invalidation();
    assert!(dimensions(left, &left_surface).is_none());
    assert!(
        runtime
            .apply(Command::View(
                left,
                ViewCommand::Load(format!("{url}?next"))
            ))
            .is_none()
    );
    let until = Instant::now() + Duration::from_secs(10);
    let mut next = None;
    while next.is_none() && Instant::now() < until {
        runtime
            .ladybird
            .wait(Some(Duration::from_millis(100)))
            .unwrap();
        for update in runtime.handle_events() {
            if let Update::Thumbnail(id, thumbnail) = update
                && id == left
            {
                next = thumbnail.get(&left_surface).map(|pixels| pixels.size());
            }
        }
    }
    runtime.close();
    std::fs::remove_dir_all(profile).unwrap();
    assert!(!samples.contains_key(&hidden));
    assert_eq!(left_size, Some((320, 180)));
    assert_eq!(right_size, Some((320, 180)));
    assert_eq!(next, left_size);
}
