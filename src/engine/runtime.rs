// SPDX-License-Identifier: GPL-3.0-only

//! Ladybird and its views, owned by the engine thread: applying commands, turning Ladybird's
//! events into updates for the tab that raised them, and copying frames out.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use elaris::downloads::{Download, DownloadId};
use elaris::ladybird::{Event, Ladybird, Waker, WebView};

use super::frames::{FrameGate, Verdict};
use super::thumbnails;

mod downloads;
mod presentation;
use downloads::configure_download_directory;
use presentation::{RETAINED_HIDDEN_FRAMES, Timing, evict_frame, eviction_enabled, show_frame};
#[cfg(test)]
mod tests;
use super::{Command, FindResult, Surface, TabUpdate, Thumbnail, Update, ViewCommand};
use crate::perf::{self, Stage};
use crate::tab::TabId;

/// How long closing keeps running Ladybird so helper processes see the views close.
const CLOSE_GRACE: Duration = Duration::from_millis(100);

/// One tab's view and where its frames go.
struct View {
    view: WebView,
    surface: Surface,
    gate: FrameGate,
    visible: bool,
    showing_fallback: bool,
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
    // Oldest hidden view first. Absent when the experiment is disabled.
    hidden_order: Option<Vec<TabId>>,
    hidden_frames_dirty: bool,
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
                hidden_order: eviction_enabled(
                    std::env::var_os("ELARIS_EVICT_HIDDEN_FRAMES").as_deref(),
                )
                .then(Vec::new),
                hidden_frames_dirty: false,
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
        let report_surfaces = perf::enabled()
            && matches!(
                command,
                Command::Open(..) | Command::Close(_) | Command::View(_, ViewCommand::Visible(_))
            );
        let update = match command {
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
        };
        if report_surfaces {
            self.report_surfaces();
        }
        update
    }

    fn report_surfaces(&self) {
        let mut visible = 0;
        let mut visible_rgba_bytes = 0;
        let mut hidden_rgba_bytes = 0;
        let mut fallback_rgba_bytes = 0;
        for view in self.views.values() {
            let (width, height) = view.surface.pixels.size();
            let bytes = u64::from(width) * u64::from(height) * 4;
            if view.visible {
                visible += 1;
            }
            if view.showing_fallback {
                fallback_rgba_bytes += bytes;
            } else if view.visible {
                visible_rgba_bytes += bytes;
            } else {
                hidden_rgba_bytes += bytes;
            }
        }
        let hidden = self.views.len() - visible;
        eprintln!(
            "[perf surfaces] visible={visible} hidden={hidden} visible_rgba_bytes={visible_rgba_bytes} hidden_rgba_bytes={hidden_rgba_bytes} fallback_rgba_bytes={fallback_rgba_bytes}"
        );
    }

    fn apply_tab_command(&mut self, command: Command) -> Option<Update> {
        let (tab, result) = match command {
            Command::Open(tab, surface) => (tab, self.open(tab, surface)),
            Command::PreflightClose(tab) => {
                return match self.views.get_mut(&tab) {
                    Some(view) => view
                        .view
                        .preflight_close()
                        .err()
                        .map(|_| Update::CloseDecision(tab, false)),
                    None => Some(Update::CloseDecision(tab, true)),
                };
            }
            Command::Close(tab) => {
                self.views.remove(&tab);
                if let Some(order) = &mut self.hidden_order {
                    order.retain(|id| *id != tab);
                }
                self.thumbnails.remove(tab);
                return None;
            }
            Command::View(tab, command) => {
                let view = self.views.get_mut(&tab)?;
                if command.invalidates_preview() {
                    self.thumbnails.remove(tab);
                }
                let visibility = match command {
                    ViewCommand::Visible(visible) if view.visible != visible => Some(visible),
                    _ => None,
                };
                let result = apply_to_view(view, command, &mut self.timing);
                if result.is_ok()
                    && let Some(visible) = visibility
                    && let Some(order) = &mut self.hidden_order
                {
                    if visible {
                        order.retain(|id| *id != tab);
                        if view.showing_fallback {
                            // Even an idle page has a displayed Ladybird frame. Restore it
                            // without waiting for a new paint notification.
                            let _ = perf::time(Stage::RestoreFrame, || {
                                show_frame(view, &mut self.timing, false)
                            });
                        }
                    } else {
                        order.push(tab);
                        self.hidden_frames_dirty = true;
                    }
                }
                (tab, result)
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
                showing_fallback: false,
                preview_revision: 0,
                preview_committed: false,
                last_thumbnail: None,
            },
        );
        Ok(())
    }

    /// Keeps full frames for the newest hidden views that have painted.
    pub(super) fn trim_hidden_frames(&mut self) {
        if !std::mem::take(&mut self.hidden_frames_dirty) {
            return;
        }
        let Some(order) = &self.hidden_order else {
            return;
        };
        let mut retained = 0;
        let mut changed = false;
        for id in order.iter().rev() {
            let Some(view) = self.views.get_mut(id) else {
                continue;
            };
            if view.showing_fallback || view.surface.pixels.size() == (0, 0) {
                continue;
            }
            retained += 1;
            if retained > RETAINED_HIDDEN_FRAMES
                && perf::time(Stage::EvictFrame, || evict_frame(view))
            {
                changed = true;
            }
        }
        if changed && perf::enabled() {
            self.report_surfaces();
        }
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
                Event::Peek { url, request } => {
                    updates.push(Update::Peek(tab, url, request));
                    continue;
                }
                Event::LoadError(error) => TabUpdate::LoadError(error),
                Event::HoveredLink(url) => TabUpdate::HoveredLink(url),
                Event::Tooltip(text) => TabUpdate::Tooltip(text),
                Event::Cursor(name) => TabUpdate::Cursor(name),
                Event::PageDialog(dialog) => TabUpdate::PageDialog(dialog),
                Event::FormPicker(picker) => {
                    updates.push(Update::FormPicker(tab, picker));
                    continue;
                }
                Event::CloseDecision(approved) => {
                    updates.push(Update::CloseDecision(tab, approved));
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
        let mut surface_size_changed = false;
        for (tab, sample) in painted {
            let Some(view) = self.views.get_mut(&tab) else {
                continue;
            };
            let new_hidden_frame = self.hidden_order.is_some()
                && !view.visible
                && !view.showing_fallback
                && view.surface.pixels.size() == (0, 0);
            let previous_size = perf::enabled().then(|| view.surface.pixels.size());
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
            self.hidden_frames_dirty |= new_hidden_frame && view.surface.pixels.size() != (0, 0);
            surface_size_changed |=
                previous_size.is_some_and(|size| size != view.surface.pixels.size());
        }
        if surface_size_changed {
            self.report_surfaces();
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
        ViewCommand::DiscardPrepared(request) => web.discard_prepared(request),
        ViewCommand::OpenContext { request, new_tab } => web.open_context_target(request, new_tab),
        ViewCommand::Find { query, request_id } => web.find(&query, request_id),
        ViewCommand::FindNext { request_id } => web.find_next(request_id),
        ViewCommand::FindPrevious { request_id } => web.find_previous(request_id),
        ViewCommand::FindEnd => web.find_end(),
        ViewCommand::ReplyToDialog {
            request_id,
            response,
        } => web.reply_to_dialog(request_id, response.as_deref()),
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
        ViewCommand::ReplyToPicker {
            request_id,
            response,
        } => web.reply_to_picker(request_id, response.as_deref()),
        ViewCommand::Focus(focused) => web.set_focus(focused),
        ViewCommand::Visible(visible) => {
            web.set_visible(visible)?;
            view.visible = visible;
            Ok(())
        }
    }
}

/// A directory from `variable`, or `fallback` within the source tree for development builds.
fn directory(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fallback))
}
