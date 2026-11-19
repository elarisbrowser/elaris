// SPDX-License-Identifier: GPL-3.0-only

//! Ladybird and its views, owned by the engine thread: applying commands, turning Ladybird's
//! events into updates for the tab that raised them, and copying frames out.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::rc::Rc;
use std::time::{Duration, Instant};

use elaris::downloads::{
    Download, DownloadId, Request as DownloadRequest, Status as DownloadStatus,
};
use elaris::ladybird::{Event, Frame, Ladybird, Waker, WebView};

use super::frames::{FrameGate, Verdict};
use super::thumbnails::{self, Sample};
use super::{
    Command, DownloadAction, FindResult, Surface, TabUpdate, Thumbnail, Update, ViewCommand,
};
use crate::perf::{self, Count, Stage};
use crate::tab::TabId;

/// How long closing keeps running Ladybird so helper processes see the views close.
const CLOSE_GRACE: Duration = Duration::from_millis(100);
const THUMBNAIL_INTERVAL: Duration = Duration::from_secs(1);

/// One tab's view and where its frames go.
struct View {
    view: WebView,
    surface: Surface,
    gate: FrameGate,
    visible: bool,
    preview_revision: u64,
    preview_committed: bool,
    last_thumbnail: Option<Instant>,
}

pub(super) struct Runtime {
    /// Declared before `ladybird`, so views are destroyed before the application.
    views: HashMap<TabId, View>,
    ladybird: Ladybird,
    /// What Ladybird's callback raised during the last wait, by view id, handled after it
    /// returns.
    events: Rc<RefCell<Vec<(u64, Event)>>>,
    /// Ladybird identifiers are private to the engine; browser IDs stay stable across commands.
    native_download_ids: HashMap<u64, DownloadId>,
    native_ids_by_download: HashMap<DownloadId, u64>,
    downloads: HashMap<DownloadId, Download>,
    timing: Timing,
    thumbnails: thumbnails::Cache,
    display_refresh_rate: f64,
}

impl Runtime {
    pub(super) fn start(
        profile: PathBuf,
        download_directory: Option<PathBuf>,
        display_refresh_rate: f64,
    ) -> Result<(Self, Option<String>), String> {
        let resources = directory("ELARIS_LADYBIRD_RESOURCE_DIR", "vendor/ladybird/Base/res");
        std::fs::create_dir_all(&profile)
            .map_err(|e| format!("cannot create profile {}: {e}", profile.display()))?;
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&events);
        let mut ladybird = Ladybird::new(&resources, &profile, move |view, event| {
            sink.borrow_mut().push((view, event));
        })?;
        let download_directory_error =
            configure_download_directory(&mut ladybird, download_directory.as_deref()).err();
        Ok((
            Self {
                views: HashMap::new(),
                ladybird,
                events,
                native_download_ids: HashMap::new(),
                native_ids_by_download: HashMap::new(),
                downloads: HashMap::new(),
                timing: Timing::default(),
                thumbnails: thumbnails::Cache::default(),
                display_refresh_rate,
            },
            download_directory_error,
        ))
    }

    /// A handle any thread can use to end a [`Self::wait`].
    pub(super) fn waker(&mut self) -> Result<Waker, String> {
        self.ladybird.waker()
    }

    /// Runs Ladybird work, blocking until there is some or a waker wakes it.
    pub(super) fn wait(&mut self) -> Result<(), String> {
        self.ladybird.wait(None)
    }

    /// Applies `command`, returning the error to report if it failed. Commands for a tab that
    /// has closed are dropped: the browser may have sent them before it closed the tab.
    pub(super) fn apply(&mut self, command: Command) -> Option<Update> {
        match command {
            Command::StartDownload(id, request) => self
                .start_download(id, request)
                .err()
                .map(|error| Update::DownloadStartFailed(id, error)),
            Command::DownloadAction(id, action) => self
                .download_action(id, action)
                .err()
                .map(|error| Update::DownloadActionFailed(id, error)),
            Command::SetDownloadDirectory(path) => {
                configure_download_directory(&mut self.ladybird, path.as_deref())
                    .err()
                    .map(Update::DownloadDirectoryFailed)
            }
            other => self.apply_tab_command(other),
        }
    }

    fn apply_tab_command(&mut self, command: Command) -> Option<Update> {
        let (tab, result) = match command {
            Command::Open(tab, surface) => (tab, self.open(tab, surface)),
            Command::Close(tab) => {
                self.views.remove(&tab);
                self.thumbnails.remove(tab);
                return None;
            }
            Command::View(tab, command) => {
                let view = self.views.get_mut(&tab)?;
                if command.invalidates_preview() {
                    self.thumbnails.remove(tab);
                }
                (tab, apply_to_view(view, command, &mut self.timing))
            }
            Command::ColorScheme(dark) => {
                return self
                    .ladybird
                    .set_color_scheme(dark)
                    .err()
                    .map(Update::Failed);
            }
            Command::Shutdown
            | Command::StartDownload(..)
            | Command::DownloadAction(..)
            | Command::SetDownloadDirectory(_) => {
                return None;
            }
        };
        result
            .err()
            .map(|error| Update::Tab(tab, TabUpdate::Error(error)))
    }

    fn start_download(&mut self, id: DownloadId, request: DownloadRequest) -> Result<(), String> {
        match self
            .ladybird
            .start_download(&request.url, request.suggested_filename.as_deref())
        {
            Ok(native_id) => {
                self.native_download_ids.insert(native_id, id);
                self.native_ids_by_download.insert(id, native_id);
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn download_action(&mut self, id: DownloadId, action: DownloadAction) -> Result<(), String> {
        match action {
            DownloadAction::Cancel => {
                let Some(native_id) = self.native_ids_by_download.get(&id).copied() else {
                    // A cancel queued behind a request that failed to start is already satisfied.
                    return Ok(());
                };
                self.ladybird.cancel_download(native_id)
            }
            DownloadAction::Open | DownloadAction::Reveal => {
                let Some(download) = self.downloads.get(&id) else {
                    return Err("download state is not available yet".to_owned());
                };
                if download.status != DownloadStatus::Completed {
                    return Err("download has not completed".to_owned());
                }
                let Some(path) = &download.destination else {
                    return Err("download destination is unavailable".to_owned());
                };
                open_download(path, action == DownloadAction::Reveal)
            }
        }
    }

    fn open(&mut self, tab: TabId, surface: Surface) -> Result<(), String> {
        let view = self
            .ladybird
            .create_view(tab.raw(), self.display_refresh_rate)?;
        self.views.insert(
            tab,
            View {
                view,
                surface,
                gate: FrameGate::default(),
                visible: true,
                preview_revision: 0,
                preview_committed: false,
                last_thumbnail: None,
            },
        );
        Ok(())
    }

    /// Turns the events Ladybird raised into updates, showing each tab's new frame if it has
    /// one. Events from views that have closed since are dropped.
    pub(super) fn handle_events(&mut self) -> Vec<Update> {
        let events = std::mem::take(&mut *self.events.borrow_mut());
        let mut updates = Vec::new();
        let mut painted: Vec<(TabId, bool)> = Vec::new();
        for (id, event) in events {
            if let Event::Download(update) = event {
                let native_id = update.native_id();
                let Some(id) = self
                    .native_download_ids
                    .get(&native_id)
                    .copied()
                    .or_else(|| DownloadId::next().ok())
                else {
                    continue;
                };
                self.native_download_ids.insert(native_id, id);
                self.native_ids_by_download.insert(id, native_id);
                self.downloads.insert(id, update.clone().into_download(id));
                updates.push(Update::Download(id, update));
                continue;
            }
            let tab = TabId::from_raw(id);
            let Some(view) = self.views.get_mut(&tab) else {
                continue;
            };
            let update = match event {
                Event::NewTab {
                    url,
                    request,
                    activate,
                } => {
                    updates.push(Update::NewTab(tab, url, request, activate));
                    continue;
                }
                Event::ContextMenu(menu) => {
                    updates.push(Update::ContextMenu(tab, menu));
                    continue;
                }
                Event::Clipboard(text) => {
                    updates.push(Update::Clipboard(tab, text));
                    continue;
                }
                Event::ImeCaret(rect) => TabUpdate::ImeCaret(rect),
                Event::Navigation(_) | Event::NavigationCommitted(_) => {
                    self.thumbnails.remove(tab);
                    view.last_thumbnail = None;
                    // A ready-to-paint notification earlier in this batch predates this event.
                    if let Some((_, sample)) = painted.iter_mut().find(|(id, _)| *id == tab) {
                        *sample = false;
                    }
                    match event {
                        Event::Navigation(url) => {
                            view.preview_committed = false;
                            TabUpdate::Navigation(url)
                        }
                        Event::NavigationCommitted(url) => {
                            view.preview_committed = true;
                            TabUpdate::NavigationCommitted(url)
                        }
                        _ => unreachable!(),
                    }
                }
                Event::Title(title) => TabUpdate::Title(title),
                Event::Loading(loading) => TabUpdate::Loading(loading),
                Event::Favicon(favicon) => TabUpdate::Favicon(favicon),
                Event::FindResult {
                    request_id,
                    current_match_index,
                    total_match_count,
                } => TabUpdate::FindResult {
                    request_id,
                    result: FindResult {
                        current_match_index,
                        total_match_count,
                    },
                },
                Event::SelectedText { request_id, text } => {
                    updates.push(Update::FindSelection(tab, request_id, text));
                    continue;
                }
                Event::History { back, forward } => TabUpdate::History { back, forward },
                Event::Download(_) => unreachable!("download events are handled above"),
                Event::ReadyToPaint => {
                    if let Some((_, sample)) = painted.iter_mut().find(|(id, _)| *id == tab) {
                        *sample = view.preview_committed;
                    } else {
                        painted.push((tab, view.preview_committed));
                    }
                    continue;
                }
            };
            updates.push(Update::Tab(tab, update));
        }
        for (tab, sample) in painted {
            let Some(view) = self.views.get_mut(&tab) else {
                continue;
            };
            match show_frame(view, &mut self.timing, sample) {
                Ok(Some(sample)) => {
                    if let Some(pixels) = self.thumbnails.store(tab, sample) {
                        updates.push(Update::Thumbnail(
                            tab,
                            Thumbnail {
                                source: view.surface.preview_source.clone(),
                                revision: view.preview_revision,
                                pixels,
                            },
                        ));
                    }
                }
                Ok(None) => {}
                Err(error) => updates.push(Update::Tab(tab, TabUpdate::Error(error))),
            }
        }
        updates
    }

    /// Closes every view, then keeps Ladybird running briefly so helper processes see them go.
    pub(super) fn close(self) {
        let Self {
            views,
            mut ladybird,
            ..
        } = self;
        drop(views);
        let deadline = Instant::now() + CLOSE_GRACE;
        let _ = ladybird.cancel_active_downloads();
        while let Some(left) = deadline.checked_duration_since(Instant::now())
            && ladybird.wait(Some(left)).is_ok()
        {}
    }
}

fn configure_download_directory(
    ladybird: &mut Ladybird,
    path: Option<&Path>,
) -> Result<(), String> {
    let Some(path) = path else {
        return ladybird.clear_download_directory();
    };
    if !path.is_absolute() {
        return Err("download directory must be an absolute path".to_owned());
    }
    std::fs::create_dir_all(path).map_err(|error| {
        format!(
            "cannot create download directory {}: {error}",
            path.display()
        )
    })?;
    ladybird.set_download_directory(path)
}

fn open_download(path: &std::path::Path, reveal: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = ProcessCommand::new("open");
        if reveal {
            command.arg("-R");
        }
        command.arg(path);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = ProcessCommand::new("xdg-open");
        command.arg(if reveal {
            path.parent().unwrap_or(path)
        } else {
            path
        });
        command
    };
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("could not open download: {error}"))
}

fn apply_to_view(view: &mut View, command: ViewCommand, timing: &mut Timing) -> Result<(), String> {
    if command.invalidates_preview() {
        // Count the applied requests, rather than reading the UI's counter, which can
        // already include commands still in flight.
        view.preview_revision += 1;
        view.preview_committed = false;
        view.last_thumbnail = None;
    }
    let web = &mut view.view;
    match command {
        ViewCommand::LoadPrepared { source, request } => web.load_prepared(source.raw(), request),
        ViewCommand::OpenContext { request, new_tab } => web.open_context_target(request, new_tab),
        ViewCommand::Find { query, request_id } => web.find(&query, request_id),
        ViewCommand::FindNext { request_id } => web.find_next(request_id),
        ViewCommand::FindPrevious { request_id } => web.find_previous(request_id),
        ViewCommand::FindEnd => web.find_end(),
        ViewCommand::SelectedText { request_id } => web.selected_text(request_id),
        ViewCommand::Edit(edit) => web.edit(&edit),
        ViewCommand::Load(url) => web.load(&url),
        ViewCommand::Reload => web.reload(),
        ViewCommand::StopLoading => web.stop_loading(),
        ViewCommand::Traverse(delta) => web.traverse_history(delta),
        ViewCommand::Resize {
            width,
            height,
            scale,
        } => {
            view.gate.laid_out_at(scale);
            timing.resized((width as u32, height as u32));
            web.resize(width, height, scale)
        }
        ViewCommand::Mouse(event) => {
            timing.input();
            web.mouse_event(event)
        }
        ViewCommand::Key(event) => {
            timing.input();
            web.key_event(event)
        }
        ViewCommand::Focus(focused) => web.set_focus(focused),
        ViewCommand::Visible(visible) => {
            view.visible = visible;
            web.set_visible(visible)
        }
    }
}

/// Shows the view's newest frame in its surface if the gate lets it through.
fn show_frame(
    view: &mut View,
    timing: &mut Timing,
    sample: bool,
) -> Result<Option<Sample>, String> {
    let Some(frame) = view.view.frame()? else {
        return Ok(None);
    };
    let skipped = match view.gate.check(frame.serial, frame.scale) {
        Verdict::Show { skipped } => skipped,
        Verdict::Duplicate => {
            perf::count(Count::Duplicate, 1);
            return Ok(None);
        }
        Verdict::Stale => {
            perf::count(Count::Stale, 1);
            return Ok(None);
        }
    };
    let rgba = perf::time(Stage::Copy, || view.view.copy_frame(&frame))?;
    let thumbnail = if sample
        && view.visible
        && view.surface.preview_source.revision() == view.preview_revision
        && view
            .last_thumbnail
            .is_none_or(|last| last.elapsed() >= THUMBNAIL_INTERVAL)
    {
        view.last_thumbnail = Some(Instant::now());
        thumbnails::sample(frame.width, frame.height, &rgba)
    } else {
        None
    };
    view.surface.count_frame();
    timing.shown(&frame, skipped);
    view.gate.shown(frame.serial);
    perf::time(Stage::Set, || {
        view.surface.pixels.set(frame.width, frame.height, rgba)
    });
    Ok(thumbnail)
}

/// Development timing (`ELARIS_PERF`): the last frame shown, the oldest input not yet on screen,
/// and the newest size not yet on screen.
#[derive(Default)]
struct Timing {
    last_frame: Option<Instant>,
    pending_input: Option<Instant>,
    pending_resize: Option<((u32, u32), Instant)>,
}

impl Timing {
    fn input(&mut self) {
        if perf::enabled() {
            self.pending_input.get_or_insert_with(Instant::now);
        }
    }

    fn resized(&mut self, size: (u32, u32)) {
        if perf::enabled() {
            self.pending_resize = Some((size, Instant::now()));
        }
    }

    fn shown(&mut self, frame: &Frame, skipped: u64) {
        if !perf::enabled() {
            return;
        }
        let now = Instant::now();
        perf::count(Count::Shown, 1);
        perf::count(Count::Skipped, skipped);
        if let Some(last) = self.last_frame.replace(now) {
            perf::record(Stage::FrameInterval, now - last);
        }
        if let Some(input) = self.pending_input.take() {
            perf::record(Stage::InputToFrame, now - input);
        }
        if let Some((size, since)) = self.pending_resize
            && size == (frame.width, frame.height)
        {
            perf::record(Stage::ResizeToFrame, now - since);
            self.pending_resize = None;
        }
    }
}

/// A directory from `variable`, or `fallback` within the source tree for development builds.
fn directory(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fallback))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a built Ladybird bridge and helper processes"]
    fn visible_views_sample_their_own_ladybird_frames() {
        let profile =
            std::env::temp_dir().join(format!("elaris-thumbnails-{}", std::process::id()));
        let (mut runtime, _) = Runtime::start(profile.clone(), None, 60.0).unwrap();
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
            assert!(runtime.apply(Command::Open(id, surface.clone())).is_none());
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
}
