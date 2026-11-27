// SPDX-License-Identifier: GPL-3.0-only

use std::time::Instant;

use super::{Browser, Polled, peek, split};
use crate::engine::{Command, Update, ViewCommand};
use crate::input;
use crate::tab::{Tab, TabId};
use elaris_core::internal::InternalRoute;
use elaris_core::perf::{self, Stage};

impl Browser {
    /// Forwards what changed to the engine and takes in what it reported. Rune calls it at the
    /// start of every frame; it never blocks.
    pub(crate) fn poll(&mut self) -> Polled {
        if perf::enabled() {
            let now = Instant::now();
            if let Some(last) = self.last_poll.replace(now) {
                perf::record(Stage::Poll, now - last);
            }
        }
        let mut polled = Polled::default();
        polled.committed.extend(self.promoted_visit.take());
        self.sync_engine();
        // Collected first: taking them in needs the browser, which the engine is part of.
        // Empty, as most polls are, it allocates nothing.
        let mut tab_ids: Vec<_> = self.tabs.iter().map(Tab::id).collect();
        if let Some(peek) = &self.peek {
            tab_ids.push(peek.page.id());
        }
        tab_ids.extend(self.fetch_ids());
        let updates: Vec<Update> = self
            .engine
            .as_ref()
            .map_or_else(Vec::new, |engine| engine.updates_for(&tab_ids));
        for update in updates {
            if let Update::Tab(id, tab_update) = &update
                && self.fetch_update(*id, tab_update)
            {
                continue;
            }
            match update {
                // A tab closed since the engine reported this has nothing to show it.
                Update::Tab(id, crate::engine::TabUpdate::WheelResult(cancelled)) => {
                    polled.changed |= self.wheel_result(id, cancelled);
                }
                Update::Tab(id, crate::engine::TabUpdate::Discarded { kept, blockers }) => {
                    if self.finish_sleep(id, kept, blockers) {
                        polled.slept.push(id);
                    }
                    polled.changed = true;
                }
                Update::Tab(id, update) => {
                    if matches!(update, crate::engine::TabUpdate::WebFullscreen(true))
                        && self.active != Some(id)
                    {
                        self.send(id, ViewCommand::ExitWebFullscreen);
                        continue;
                    }
                    if let Some(peek) = self.peek.as_mut().filter(|peek| peek.page.id() == id) {
                        if !matches!(&update,
                            crate::engine::TabUpdate::Navigation(url)
                            | crate::engine::TabUpdate::NavigationCommitted(url)
                            if InternalRoute::is_internal_address(url))
                        {
                            if let crate::engine::TabUpdate::LoadError(error) = update {
                                peek.page.error = Some(error);
                                peek.page.loading = false;
                                polled.changed = true;
                            } else {
                                polled.changed |= peek.page.update(update);
                            }
                        }
                        continue;
                    }
                    if self.tab(id).is_some_and(Tab::is_internal) {
                        continue;
                    }
                    let internal_navigation = match &update {
                        crate::engine::TabUpdate::Navigation(url)
                        | crate::engine::TabUpdate::NavigationCommitted(url) => {
                            InternalRoute::is_internal_address(url)
                        }
                        _ => false,
                    };
                    if internal_navigation {
                        if let Some(tab) = self.tab_mut(id) {
                            tab.error = Some(
                                "Internal pages can only be opened from Elaris navigation".into(),
                            );
                            polled.changed = true;
                        }
                        continue;
                    }
                    if matches!(
                        &update,
                        crate::engine::TabUpdate::Navigation(_)
                            | crate::engine::TabUpdate::NavigationCommitted(_)
                    ) {
                        self.invalidate_find(id);
                    }
                    if let crate::engine::TabUpdate::FindResult { request_id, .. } = &update
                        && self
                            .tab(id)
                            .is_some_and(|tab| tab.find.request_id() != *request_id)
                    {
                        continue;
                    }
                    if matches!(&update, crate::engine::TabUpdate::PageDialog(Some(dialog))
                        if dialog.kind == elaris::ladybird::PageDialogKind::BeforeUnload)
                        && self.tab(id).is_some_and(|tab| tab.awaiting_close)
                    {
                        self.dismiss_peek();
                        self.activate_tab(id);
                    }
                    if let Some(tab) = self.tab_mut(id) {
                        if matches!(
                            update,
                            crate::engine::TabUpdate::LoadError(_)
                                | crate::engine::TabUpdate::Crashed(Some(_))
                        ) {
                            tab.awaiting_close = false;
                        }
                        let committed = match &update {
                            crate::engine::TabUpdate::NavigationCommitted(url) => Some(url.clone()),
                            _ => None,
                        };
                        let title_changed = matches!(&update, crate::engine::TabUpdate::Title(_));
                        polled.changed |= tab.update(update);
                        if let Some(url) = committed {
                            polled.committed.push((id, url, tab.title.clone()));
                        }
                        if title_changed
                            && !tab.committed_url.is_empty()
                            && tab.url == tab.committed_url
                        {
                            polled
                                .titles
                                .push((tab.committed_url.clone(), tab.title.clone()));
                        }
                    }
                }
                Update::PermissionSaved(origin, kind, state) => {
                    polled.permission_choices.push((origin, kind, state));
                    polled.changed = true;
                }
                Update::FormPicker(id, picker) => {
                    polled.form_pickers.push((id, picker));
                    polled.changed = true;
                }
                Update::CloseDecision(id, approved) => {
                    if let Some(tab) = self.tab_mut(id)
                        && std::mem::take(&mut tab.awaiting_close)
                    {
                        polled.close_decisions.push((id, approved));
                        polled.changed = true;
                    }
                }
                Update::Thumbnail(id, thumbnail) => {
                    if let Some(tab) = self.tab_mut(id)
                        && !tab.is_internal()
                        && thumbnail.get(&tab.surface).is_some()
                    {
                        tab.thumbnail = Some(thumbnail);
                        polled.thumbnails_ready.push(id);
                    }
                }
                Update::Peek(source, url, request) => {
                    polled.changed |= self.open_peek(source, url, request);
                }
                Update::Popup(source, request, activate) => {
                    if self.page(source).is_some_and(|tab| !tab.is_internal()) {
                        polled.popups.push((source, request, activate));
                    }
                }
                Update::Screenshot(id, result) => {
                    polled.screenshots.push((id, result));
                    polled.changed = true;
                }
                Update::PopupClosed(id) => {
                    self.close_tab(id);
                    polled.changed = true;
                }
                Update::NewTab(source, url, request, activate) => {
                    if self.tab(source).is_some_and(|tab| !tab.is_internal())
                        && !InternalRoute::is_internal_address(&url)
                    {
                        // Ladybird already parsed and authorized this navigation. Keep its referrer
                        // and origin by transferring the descriptor instead of re-loading the URL.
                        let tab = self.insert_tab(url);
                        if let Some(tab) = self.tab_mut(tab) {
                            tab.engine.pending_load = None;
                        }
                        self.send(tab, ViewCommand::LoadPrepared { source, request });
                        if activate {
                            self.activate_tab(tab);
                        }
                        polled.changed = true;
                    } else {
                        self.send(source, ViewCommand::DiscardPrepared(request));
                    }
                }
                Update::FindSelection(id, request_id, text)
                    if self.tab(id).is_some_and(|tab| !tab.is_internal()) =>
                {
                    polled.find_selections.push((id, request_id, text));
                    polled.changed = true;
                }
                Update::FindSelection(..) => {}
                Update::ContextMenu(id, menu) if self.active == Some(id) => {
                    polled.menu = Some((id, menu));
                    polled.changed = true;
                }
                Update::Clipboard(id, text) if self.input_page() == Some(id) => {
                    polled.clipboard = Some(text)
                }
                Update::ContextMenu(..) | Update::Clipboard(..) => {}
                Update::Failed(error) => {
                    self.fail(error);
                    polled.changed = true;
                }
                Update::Download(id, update) => {
                    self.apply_download_update(id, update);
                    polled.changed = true;
                }
                Update::DownloadStartFailed(id, error) => {
                    self.fail_download_start(id, error);
                    polled.changed = true;
                }
                Update::DownloadActionFailed(id, error) => {
                    self.set_download_error(id, error);
                    polled.changed = true;
                }
                Update::DownloadDirectoryFailed(error) => {
                    polled.download_directory_error = Some(error);
                    polled.changed = true;
                }
                Update::Processes(id, processes) => {
                    polled.processes = Some((id, processes));
                    polled.changed = true;
                }
                Update::EndProcessFailed(id, error) => {
                    polled.processes_error = Some((id, error));
                    polled.changed = true;
                }
            }
        }
        polled.fetched = self.finish_fetches();
        polled.changed |= !polled.fetched.is_empty();
        for id in self.visible_tabs().into_iter().flatten() {
            if let Some(tab) = self.tab_mut(id) {
                let frames = tab.surface.frames();
                polled.animating |= std::mem::replace(&mut tab.engine.frames, frames) != frames;
            }
        }
        if let Some(peek) = &mut self.peek {
            let frames = peek.page.surface.frames();
            polled.animating |= std::mem::replace(&mut peek.page.engine.frames, frames) != frames;
        }
        let downloads = self.downloads();
        if self.last_download_snapshot != downloads {
            self.last_download_snapshot = downloads;
            polled.changed = true;
            polled.downloads_changed = true;
        }
        polled
    }

    pub(crate) fn set_hosted_pages_allowed(&mut self, allowed: bool) {
        self.hosted_pages_allowed = allowed;
    }

    /// Asks the engine to describe its processes for the Task Manager in `tab`. Returns false
    /// without an engine, which has no processes to describe.
    pub(crate) fn request_processes(&self, tab: TabId) -> bool {
        self.engine
            .as_ref()
            .map(|engine| engine.send(Command::Processes(tab)))
            .is_some()
    }

    /// Ends the engine process with Ladybird's child ID `id`, for the Task Manager in `tab`.
    pub(crate) fn end_process(&self, tab: TabId, id: u64) {
        if let Some(engine) = &self.engine {
            engine.send(Command::EndProcess(tab, id));
        }
    }

    pub(super) fn send_color_scheme(&self) {
        if let Some(engine) = &self.engine
            && let Some(dark) = self.window.dark.get()
            && self.window.applied_dark.replace(Some(dark)) != Some(dark)
        {
            engine.send(Command::ColorScheme(dark));
        }
    }

    /// Tells the engine what changed about each tab: whether it is shown and focused, the
    /// viewport it lays out in, and its first page.
    pub(super) fn sync_engine(&mut self) {
        // Before any first load, so the first page lays out in the right scheme.
        self.send_color_scheme();
        let Some(engine) = &self.engine else {
            return;
        };
        let window = &self.window;
        let scale = window.scale.get();
        let panes = self.visible_tabs();
        let split_ratio = self.displayed_split().map(|split| split.ratio);
        let input_page = self.input_page();
        let peek_id = self.peek.as_ref().map(|peek| peek.page.id());
        let hosted_pages_allowed = self.hosted_pages_allowed;
        // Release the old focus before granting it to a different view.
        for tab in self
            .tabs
            .iter_mut()
            .chain(self.peek.iter_mut().map(|peek| &mut peek.page))
        {
            if tab.engine.focused && input_page != Some(tab.id()) {
                tab.engine.focused = false;
                engine.send(Command::View(tab.id(), ViewCommand::Focus(false)));
            }
        }
        let scheme_known = window.applied_dark.get().is_some();
        let (private, accessibility) = (self.private, self.accessibility);
        let default_zoom = self.default_zoom;
        let now = Instant::now();
        for tab in self
            .tabs
            .iter_mut()
            .chain(self.peek.iter_mut().map(|peek| &mut peek.page))
        {
            if tab.is_internal() {
                continue;
            }
            let id = tab.id();
            // A Peek card scales its page, which a layer of its own can't follow; once promoted
            // to a tab, the page moves to one.
            let hosted = should_host_layer(
                hosted_pages_allowed,
                peek_id == Some(id),
                tab.surface.hosts_layer(),
                window.suspended,
            );
            let active = input_page == Some(id);
            let side = panes.iter().position(|pane| *pane == Some(id));
            let is_peek = peek_id == Some(id);
            let visible = !window.suspended && (is_peek || side.is_some());
            if visible {
                tab.last_seen = now;
                tab.kept_awake.clear();
                if tab.asleep {
                    wake(engine, tab, private, accessibility, default_zoom);
                }
            }
            if tab.asleep {
                continue;
            }
            let applied = &mut tab.engine;
            let size = if is_peek {
                peek::geometry(window.viewport).page_size()
            } else if let (Some(side), Some(ratio)) = (side, split_ratio)
                && panes.len() == 2
            {
                [
                    split::widths(window.viewport[0], ratio)[side],
                    window.viewport[1],
                ]
            } else {
                side.zip(split_ratio)
                    .map_or(window.viewport, |(side, ratio)| {
                        let [width, height] = window.viewport;
                        let [_, _, width, height] = split::grid_cell_rect(
                            [0.0, 0.0, width, height],
                            panes.len(),
                            side,
                            ratio,
                        );
                        [width, height]
                    })
            };
            let [width, height] = size.map(|points| input::device(points, scale));
            let viewport = (width > 0 && height > 0).then_some((width, height, f64::from(scale)));
            let send = |command| engine.send(Command::View(id, command));
            if applied.hidden == visible {
                applied.hidden = !visible;
                send(ViewCommand::Visible(visible));
            }
            if applied.hosted != hosted {
                applied.hosted = hosted;
                send(ViewCommand::HostLayer(hosted));
            }
            // Hidden tabs keep their last layout, unless they have yet to load their first page.
            if (visible || (applied.pending_load.is_some() && !applied.deferred_load))
                && let Some(wanted) = viewport
                && applied.viewport != Some(wanted)
            {
                let (width, height, scale) = wanted;
                send(ViewCommand::Resize {
                    width,
                    height,
                    scale,
                });
                applied.viewport = Some(wanted);
            }
            // The first load waits for a viewport and the colour scheme, so the page lays out
            // at its real size and never starts in the wrong scheme.
            if applied.viewport.is_some()
                && scheme_known
                && (visible || !applied.deferred_load)
                && let Some(url) = applied.pending_load.take()
            {
                tab.surface.request_preview_invalidation();
                send(if std::mem::take(&mut applied.restore) {
                    ViewCommand::Restore(url)
                } else {
                    ViewCommand::Load(url)
                });
            }
            let focused = !window.suspended && active && window.focused && window.viewport_focused;
            if applied.focused != focused {
                applied.focused = focused;
                send(ViewCommand::Focus(focused));
            }
        }
    }
}

fn should_host_layer(
    hosted_pages_allowed: bool,
    is_peek: bool,
    surface_can_host: bool,
    window_suspended: bool,
) -> bool {
    hosted_pages_allowed
        && !is_peek
        && surface_can_host
        // A macOS CALayerHost holds the page's only visible frame. Keep it attached while a
        // native popup temporarily makes the window resign key; visibility still pauses painting.
        && (cfg!(target_os = "macos") || !window_suspended)
}

/// Opens a sleeping tab's view again, as it was set up before it slept. Its page loads once the
/// view has a viewport, as a first page does.
fn wake(
    engine: &crate::engine::Engine,
    tab: &mut Tab,
    private: bool,
    accessibility: bool,
    zoom: u16,
) {
    let id = tab.id();
    tab.asleep = false;
    engine.send(Command::Open(id, tab.surface.clone(), private));
    let zoom = tab.zoom_override.unwrap_or(zoom);
    if zoom != 100 {
        engine.send(Command::View(id, ViewCommand::SetZoom(zoom)));
    }
    if accessibility {
        engine.send(Command::View(id, ViewCommand::SetAccessibility(true)));
    }
    tab.engine.deferred_load = false;
}

#[cfg(test)]
mod tests {
    use super::should_host_layer;

    #[test]
    fn suspension_keeps_a_mac_page_layer_attached() {
        assert_eq!(
            should_host_layer(true, false, true, true),
            cfg!(target_os = "macos")
        );
    }

    #[test]
    fn page_layer_requirements_still_apply_while_suspended() {
        assert!(!should_host_layer(false, false, true, true));
        assert!(!should_host_layer(true, true, true, true));
        assert!(!should_host_layer(true, false, false, true));
    }
}
