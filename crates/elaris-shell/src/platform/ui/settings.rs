//! The browser's settings, shared by every window and saved to the profile.
//!
//! Views read settings with [`Settings::get`] and change them with
//! [`Settings::update`], which notifies everything observing the `Settings`
//! global, so every window follows a change. Changes are saved a moment
//! after they happen, off the main thread, and once more when the app quits.
//! Edits to the profile's config file are picked up while the app is running.

use elaris_core::SearchEngines;
use elaris_core::{BrowserSettings, Feature, SpaceData, SpaceId, ThemeMode};
use elaris_storage::{Profile, ProfileWriteTicket, SpaceProfile};
use gpui::{App, BorrowAppContext, Global, Task, WindowAppearance};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::super::app_update::update_app;
use super::super::trace;
use super::motion::{MotionConfig, MotionTimings};
use super::workspace_layout::WorkspaceLayout;

/// How long after a change the settings are saved, so a burst of changes,
/// as while dragging the sidebar's edge, saves once.
const SAVE_DELAY: Duration = Duration::from_millis(500);
/// How often the profile config is checked for edits made outside Photon.
const CONFIG_RELOAD_INTERVAL: Duration = Duration::from_secs(1);
/// Wait for an editor's write to settle before parsing its file.
const CONFIG_RELOAD_SETTLE: Duration = Duration::from_millis(100);
/// Keep the loading chip visible long enough to read on fast reloads.
const CONFIG_RELOAD_MINIMUM_DURATION: Duration = Duration::from_millis(250);
/// How long the completed config reload chip remains visible.
const CONFIG_RELOAD_NOTICE_DURATION: Duration = Duration::from_secs(3);
static NEXT_TEMPORARY_SPACE_PROFILE: AtomicU64 = AtomicU64::new(0);

type UiSettingsRevision = Arc<Mutex<Option<u64>>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConfigReloadNotice {
    Reloading,
    Reloaded(Instant),
    Failed(Instant),
}

/// A short-lived root for Engine data when Photon itself has no saved profile.
/// The runtime holds a clone so it cannot be removed while Engine sessions use it.
#[derive(Clone)]
pub(in crate::platform) struct TemporaryProfileRoot(Arc<TemporaryProfileDirectory>);

struct TemporaryProfileDirectory {
    root: PathBuf,
}

impl TemporaryProfileRoot {
    fn create() -> io::Result<Self> {
        let temporary = std::env::temp_dir();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for _ in 0..100 {
            let sequence = NEXT_TEMPORARY_SPACE_PROFILE.fetch_add(1, Ordering::Relaxed);
            let root = temporary.join(format!(
                "elaris-temporary-profile-{}-{timestamp}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&root) {
                Ok(()) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
                    }
                    return Ok(Self(Arc::new(TemporaryProfileDirectory { root })));
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a temporary Photon profile",
        ))
    }

    fn space_engine_dir(&self, space: SpaceId) -> io::Result<PathBuf> {
        let path = self
            .0
            .root
            .join("Spaces")
            .join(space.to_string())
            .join("Engine");
        fs::create_dir_all(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(path)
    }
}

impl Drop for TemporaryProfileDirectory {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.root)
            && error.kind() != io::ErrorKind::NotFound
        {
            eprintln!("Photon: could not remove temporary profile: {error}");
        }
    }
}

/// The settings, and where they are saved; `None` keeps them in memory only.
pub(super) struct Settings {
    settings: BrowserSettings,
    profile: Option<Profile>,
    /// Stable write handles for each space's separate browsing data file.
    space_profiles: HashMap<SpaceId, SpaceProfile>,
    /// Per-space Engine directories for an unsaved temporary profile.
    temporary_profile: Option<TemporaryProfileRoot>,
    /// The pending save.
    save: Option<Task<()>>,
    /// Watches the profile config for edits made outside Photon.
    _reload: Option<Task<()>>,
    /// The latest config revision written by settings controls in Photon.
    ui_settings_revision: UiSettingsRevision,
    config_reload_notice: Option<ConfigReloadNotice>,
    /// The workspace layout the settings resolve to in each space, worked
    /// out on first use after each change, as drawing asks for it many
    /// times a frame.
    layout: RefCell<HashMap<SpaceId, Rc<WorkspaceLayout>>>,
}

impl Global for Settings {}

/// Installs settings already loaded off the application thread and keeps them
/// saved in `profile`.
#[cfg(test)]
pub(in crate::platform) fn load_settings(
    profile: Option<Profile>,
    settings: BrowserSettings,
    revision: Option<u64>,
    cx: &mut App,
) {
    load_settings_with_space_profiles(profile, settings, revision, Vec::new(), cx);
}

pub(in crate::platform) fn load_settings_with_space_profiles(
    profile: Option<Profile>,
    mut settings: BrowserSettings,
    revision: Option<u64>,
    space_profiles: Vec<(SpaceId, SpaceProfile)>,
    cx: &mut App,
) {
    let ui_settings_revision = Arc::new(Mutex::new(None));
    let temporary_profile = profile.is_none().then(|| {
        TemporaryProfileRoot::create()
            .unwrap_or_else(|error| panic!("could not create temporary Photon profile: {error}"))
    });
    settings = normalize_settings(settings);
    let reload = profile
        .clone()
        .map(|profile| watch_settings(profile, revision, ui_settings_revision.clone(), cx));
    let space_profiles = space_profiles.into_iter().collect();
    cx.set_global(Settings {
        settings,
        profile,
        space_profiles,
        temporary_profile,
        save: None,
        _reload: reload,
        ui_settings_revision: ui_settings_revision.clone(),
        config_reload_notice: None,
        layout: RefCell::default(),
    });
    cx.on_app_quit(|cx| {
        let save = cx.update_global::<Settings, _>(|store, _| {
            store.save = None;
            store
                .snapshot()
                .map(|snapshot| (snapshot, store.ui_settings_revision.clone()))
        });
        async move {
            if let Some((snapshot, ui_settings_revision)) = save {
                write_settings(snapshot, &ui_settings_revision);
            }
        }
    })
    .detach();
}

fn normalize_settings(mut settings: BrowserSettings) -> BrowserSettings {
    // A search engine this build does not know falls back to the default.
    if SearchEngines::builtin()
        .get(&settings.default_search_engine)
        .is_none()
    {
        settings.default_search_engine = BrowserSettings::default().default_search_engine;
    }
    settings
}

fn file_stamp(path: &std::path::Path) -> Option<(std::time::SystemTime, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

fn watch_settings(
    profile: Profile,
    mut revision: Option<u64>,
    ui_settings_revision: UiSettingsRevision,
    cx: &mut App,
) -> Task<()> {
    cx.spawn(async move |cx| {
        let settings_path = profile.settings_path().to_path_buf();
        let mut last_stamp = None;
        loop {
            cx.background_executor().timer(CONFIG_RELOAD_INTERVAL).await;
            // A metadata check is far cheaper than reading and hashing the
            // file, so only an mtime or size change leads to a full read.
            let path = settings_path.clone();
            let stamp = cx
                .background_executor()
                .spawn(async move { file_stamp(&path) })
                .await;
            if stamp.is_some() && stamp == last_stamp {
                continue;
            }
            last_stamp = stamp;
            let profile_to_check = profile.clone();
            let Some(candidate) = cx
                .background_executor()
                .spawn(async move { profile_to_check.settings_revision().ok() })
                .await
            else {
                continue;
            };
            if revision == Some(candidate) {
                continue;
            }

            cx.background_executor().timer(CONFIG_RELOAD_SETTLE).await;
            let profile_to_check = profile.clone();
            let confirmed = cx
                .background_executor()
                .spawn(async move { profile_to_check.settings_revision().ok() })
                .await;
            if confirmed != Some(candidate) {
                continue;
            }

            let is_ui_settings_write = {
                let mut ui_revision = ui_settings_revision
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if *ui_revision == Some(candidate) {
                    *ui_revision = None;
                    true
                } else {
                    false
                }
            };
            if is_ui_settings_write {
                revision = Some(candidate);
                continue;
            }

            // Remember invalid edits too, and retry only after the file changes
            // again. This keeps an in-progress or malformed edit from replacing
            // the last settings that were loaded successfully.
            revision = Some(candidate);
            let profile_to_prepare = profile.clone();
            if update_app(cx, move |cx| {
                Settings::prepare_reload(&profile_to_prepare, cx);
            })
            .await
            .is_none()
            {
                break;
            }
            let reloading_started = Instant::now();
            let profile_to_reload = profile.clone();
            match cx
                .background_executor()
                .spawn(async move {
                    let (mut settings, revision) = profile_to_reload.reload_settings()?;
                    let space_ids: Vec<_> =
                        settings.spaces.all().iter().map(|space| space.id).collect();
                    let mut space_profiles = Vec::with_capacity(space_ids.len());
                    for space_id in space_ids {
                        let profile = profile_to_reload.space(space_id)?;
                        if let Some(space) = settings.spaces.get_mut(space_id) {
                            space.set_data(profile.load_space_data());
                        }
                        space_profiles.push((space_id, profile));
                    }
                    Ok::<_, io::Error>((settings, revision, space_profiles))
                })
                .await
            {
                Ok((settings, loaded_revision, space_profiles)) => {
                    revision = Some(loaded_revision);
                    let remaining =
                        CONFIG_RELOAD_MINIMUM_DURATION.saturating_sub(reloading_started.elapsed());
                    if !remaining.is_zero() {
                        cx.background_executor().timer(remaining).await;
                    }
                    if update_app(cx, move |cx| {
                        Settings::apply_reload(settings, space_profiles, cx);
                    })
                    .await
                    .is_none()
                    {
                        break;
                    }
                }
                Err(error) => {
                    trace(format_args!("reloading config.toml: {error}"));
                    if update_app(cx, Settings::fail_reload).await.is_none() {
                        break;
                    }
                }
            }
        }
    })
}

impl Settings {
    fn prepare_reload(profile: &Profile, cx: &mut App) {
        profile.begin_settings_write();
        cx.update_global::<Self, _>(|store, _| {
            store.save = None;
            store.config_reload_notice = Some(ConfigReloadNotice::Reloading);
        });
    }

    fn apply_reload(
        settings: BrowserSettings,
        space_profiles: Vec<(SpaceId, SpaceProfile)>,
        cx: &mut App,
    ) {
        let mut settings = normalize_settings(settings);
        cx.update_global::<Self, _>(|store, _| {
            // A file reload supersedes any unsaved in-memory changes and their
            // pending write; the profile write ticket was already invalidated.
            store.save = None;
            let mut current_space_profiles = HashMap::new();
            for (space_id, loaded_profile) in space_profiles {
                let profile = store
                    .space_profiles
                    .get(&space_id)
                    .cloned()
                    .unwrap_or(loaded_profile);
                if let (Some(space), Some(current)) = (
                    settings.spaces.get_mut(space_id),
                    store.settings.spaces.get(space_id),
                ) {
                    // A settings-file edit must not discard newer space data
                    // that is still waiting for its own snapshot to reach disk.
                    space.set_data(current.data());
                }
                current_space_profiles.insert(space_id, profile);
            }
            store.space_profiles = current_space_profiles;
            if store.settings != settings {
                store.settings = settings;
                store.layout = RefCell::default();
            }
            store.config_reload_notice = Some(ConfigReloadNotice::Reloaded(Instant::now()));
        });
    }

    fn fail_reload(cx: &mut App) {
        cx.update_global::<Self, _>(|store, _| {
            store.config_reload_notice = Some(ConfigReloadNotice::Failed(Instant::now()));
        });
    }

    pub(super) fn config_reload_notice(cx: &App) -> Option<ConfigReloadNotice> {
        cx.global::<Self>().config_reload_notice
    }

    pub(super) fn config_reload_notice_duration() -> Duration {
        CONFIG_RELOAD_NOTICE_DURATION
    }

    /// The reusable motion framework's view of the saved and system settings.
    pub(super) fn motion(cx: &App) -> MotionConfig {
        let settings = Self::get(cx).animations;
        MotionConfig {
            enabled: Self::get(cx).is_feature_enabled(Feature::Animations),
            reduce_motion: cx.reduce_motion(),
            effect_strength: f32::from(settings.effect_strength_percent.min(300)) / 100.0,
            timings: MotionTimings {
                quick: Duration::from_millis(u64::from(settings.quick_duration_ms.min(2_000))),
                standard: Duration::from_millis(u64::from(
                    settings.standard_duration_ms.min(2_000),
                )),
                gentle: Duration::from_millis(u64::from(settings.gentle_duration_ms.min(2_000))),
            },
        }
    }

    pub(super) fn get(cx: &App) -> &BrowserSettings {
        &cx.global::<Self>().settings
    }

    pub(super) fn active_space_id(cx: &App) -> SpaceId {
        Self::get(cx).spaces.active().id
    }

    pub(super) fn space_engine_dir(space: SpaceId, cx: &App) -> io::Result<Option<PathBuf>> {
        let store = cx.global::<Self>();
        if let Some(profile) = &store.profile {
            return profile.space(space).map(|space| Some(space.engine_dir()));
        }
        store
            .temporary_profile
            .as_ref()
            .map(|profile| profile.space_engine_dir(space))
            .transpose()
    }

    #[cfg(not(feature = "ui-only"))]
    pub(in crate::platform) fn temporary_profile(cx: &App) -> Option<TemporaryProfileRoot> {
        cx.global::<Self>().temporary_profile.clone()
    }

    /// Whether changes are saved between launches.
    pub(super) fn is_saved(cx: &App) -> bool {
        cx.global::<Self>().profile.is_some()
    }

    pub(super) fn config_path(cx: &App) -> Option<std::path::PathBuf> {
        cx.global::<Self>()
            .profile
            .as_ref()
            .map(|profile| profile.settings_path().to_path_buf())
    }

    /// Changes the settings, notifies observers, and saves them shortly.
    pub(super) fn update(cx: &mut App, update: impl FnOnce(&mut BrowserSettings)) {
        cx.update_global::<Self, _>(|store, cx| {
            update(&mut store.settings);
            store.layout = RefCell::default();
            store.save_soon(cx);
        });
    }

    /// Saves the settings a moment from now, unless a save is waiting.
    fn save_soon(&mut self, cx: &mut App) {
        if self.profile.is_none() || self.save.is_some() {
            return;
        }
        self.save = Some(cx.spawn(async move |cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let save = update_app(cx, |cx| {
                cx.update_global::<Self, _>(|store, _| {
                    store.save = None;
                    store
                        .snapshot()
                        .map(|snapshot| (snapshot, store.ui_settings_revision.clone()))
                })
            })
            .await
            .flatten();
            // Clearing `save` above dropped this task's handle, which
            // cancels it at its next await, so the write runs detached.
            if let Some((snapshot, ui_settings_revision)) = save {
                cx.background_executor()
                    .spawn(async move { write_settings(snapshot, &ui_settings_revision) })
                    .detach();
            }
        }));
    }

    /// The workspace layout in `space`, resolved by `resolve` once per
    /// change.
    pub(super) fn layout(
        space: SpaceId,
        cx: &App,
        resolve: impl FnOnce(&BrowserSettings, SpaceId) -> WorkspaceLayout,
    ) -> Rc<WorkspaceLayout> {
        let store = cx.global::<Self>();
        store
            .layout
            .borrow_mut()
            .entry(space)
            .or_insert_with(|| Rc::new(resolve(&store.settings, space)))
            .clone()
    }

    fn snapshot(&mut self) -> Option<SettingsSnapshot> {
        let profile = self.profile.clone()?;
        let ticket = profile.begin_settings_write();
        let mut settings = self.settings.clone();
        let mut space_data = Vec::new();
        for space in self.settings.spaces.all() {
            let id = space.id;
            let Some(data) = self.settings.spaces.get(id).map(|space| space.data()) else {
                continue;
            };
            let space_profile = if let Some(profile) = self.space_profiles.get(&id) {
                profile.clone()
            } else {
                let Ok(profile) = profile.space(id) else {
                    eprintln!(
                        "Photon: could not open profile data for space {id}; settings were not saved"
                    );
                    return None;
                };
                self.space_profiles.insert(id, profile.clone());
                profile
            };
            let data_ticket = space_profile.begin_space_data_write();
            space_data.push((space_profile, data, data_ticket));
            if let Some(space) = settings.spaces.get_mut(id) {
                space.clear_data();
            }
        }
        settings.new_tab.pinned.clear();
        settings.new_tab.hidden.clear();
        settings.sidebar.favourites.clear();
        Some(SettingsSnapshot {
            profile,
            settings,
            ticket,
            space_data,
        })
    }

    /// The search engines, with the chosen one as the default.
    pub(super) fn search_engines(cx: &App) -> SearchEngines {
        let mut engines = SearchEngines::builtin();
        engines.set_default(&Self::get(cx).default_search_engine);
        engines
    }

    /// The appearance pages and menus use: the chosen one, or the system's.
    pub(super) fn appearance(system: WindowAppearance, cx: &App) -> WindowAppearance {
        Self::chosen_appearance(cx).unwrap_or(system)
    }

    /// The chosen appearance, or `None` to follow the system.
    pub(super) fn chosen_appearance(cx: &App) -> Option<WindowAppearance> {
        match Self::get(cx).theme {
            ThemeMode::System => None,
            ThemeMode::Light => Some(WindowAppearance::Light),
            ThemeMode::Dark => Some(WindowAppearance::Dark),
        }
    }
}

struct SettingsSnapshot {
    profile: Profile,
    settings: BrowserSettings,
    ticket: ProfileWriteTicket,
    space_data: Vec<(SpaceProfile, SpaceData, ProfileWriteTicket)>,
}

fn write_settings(snapshot: SettingsSnapshot, ui_settings_revision: &UiSettingsRevision) {
    let SettingsSnapshot {
        profile,
        settings,
        ticket,
        space_data,
    } = snapshot;
    for (space_profile, data, ticket) in space_data {
        if let Err(error) = space_profile.save_space_data_if_current(ticket, &data) {
            eprintln!("failed to save space data: {error}");
            return;
        }
    }
    match profile.save_settings_if_current(ticket, &settings) {
        Ok(true) => remember_ui_settings_revision(&profile, ui_settings_revision),
        Ok(false) => {}
        Err(error) => eprintln!("failed to save settings: {error}"),
    }
}

fn remember_ui_settings_revision(profile: &Profile, ui_settings_revision: &UiSettingsRevision) {
    if let Ok(revision) = profile.settings_revision() {
        *ui_settings_revision
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(revision);
    }
}
