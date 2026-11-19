// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Paths {
    pub(crate) application_dir: PathBuf,
    pub(crate) profile_dir: PathBuf,
    pub(crate) history_file: PathBuf,
    pub(crate) session_file: PathBuf,
    pub(crate) config_file: PathBuf,
}

impl Paths {
    pub(crate) fn resolve() -> io::Result<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg_data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from);
        let xdg_config_home = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from);
        let data_dir = std::env::var_os("ELARIS_DATA_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        let explicit_profile_dir = std::env::var_os("ELARIS_LADYBIRD_PROFILE_DIR")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from);
        #[cfg(target_os = "macos")]
        let platform = Platform::MacOs;
        #[cfg(not(target_os = "macos"))]
        let platform = Platform::Linux;

        Self::resolve_with(
            platform,
            data_dir.as_deref(),
            home.as_deref(),
            xdg_data_home.as_deref(),
            xdg_config_home.as_deref(),
            explicit_profile_dir.as_deref(),
            cfg!(debug_assertions),
        )
    }

    pub(super) fn resolve_with(
        platform: Platform,
        data_dir: Option<&Path>,
        home: Option<&Path>,
        xdg_data_home: Option<&Path>,
        xdg_config_home: Option<&Path>,
        explicit_profile_dir: Option<&Path>,
        development: bool,
    ) -> io::Result<Self> {
        let application_dir = if let Some(data_dir) = data_dir {
            if !data_dir.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "ELARIS_DATA_DIR must be an absolute path",
                ));
            }
            data_dir.to_path_buf()
        } else {
            application_dir(platform, home, xdg_data_home, development).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "cannot locate Elaris data directory",
                )
            })?
        };
        let profile_dir = profile_directory(&application_dir, explicit_profile_dir);
        let config_dir = if data_dir.is_some() {
            application_dir.clone()
        } else {
            config_directory(platform, home, xdg_config_home, development).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "cannot locate Elaris configuration directory",
                )
            })?
        };
        Ok(Self {
            history_file: application_dir.join("history.sqlite"),
            session_file: application_dir.join("session.json"),
            config_file: config_dir.join("config.toml"),
            application_dir,
            profile_dir,
        })
    }

    pub(super) fn create_directories(&self) -> io::Result<()> {
        create_private_dir(&self.application_dir)?;
        create_private_dir(&self.profile_dir)?;
        if let Some(parent) = self.config_file.parent() {
            create_private_dir(parent)?;
        }
        Ok(())
    }
}

pub(crate) fn default_download_directory() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    #[cfg(target_os = "linux")]
    if let Some(path) = xdg_download_directory(
        &home,
        std::env::var_os("XDG_DOWNLOAD_DIR").as_deref(),
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .as_deref(),
    ) {
        return path;
    }
    home.join("Downloads")
}

#[cfg(target_os = "linux")]
fn xdg_download_directory(
    home: &Path,
    environment_path: Option<&std::ffi::OsStr>,
    config_home: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = environment_path.map(PathBuf::from)
        && path.is_absolute()
    {
        return Some(path);
    }
    let default_config_home = home.join(".config");
    let config_home = config_home
        .filter(|path| path.is_absolute())
        .unwrap_or(&default_config_home);
    let contents = fs::read_to_string(config_home.join("user-dirs.dirs")).ok()?;
    contents
        .lines()
        .filter_map(|line| parse_xdg_download_directory(line, home))
        .next_back()
}

#[cfg(any(target_os = "linux", test))]
pub(super) fn parse_xdg_download_directory(line: &str, home: &Path) -> Option<PathBuf> {
    let value = line
        .strip_prefix("XDG_DOWNLOAD_DIR")?
        .trim_start()
        .strip_prefix('=')?
        .trim_start()
        .strip_prefix('"')?;
    let (prefix, rest) = value
        .strip_prefix("$HOME")
        .map(|rest| (Some(home), rest))
        .or_else(|| value.starts_with('/').then_some((None, value)))?;
    let mut path = prefix
        .map(|home| home.as_os_str().to_os_string())
        .unwrap_or_default();
    let mut chars = rest.chars();
    let mut closed = false;
    while let Some(character) = chars.next() {
        match character {
            '"' => {
                closed = true;
                break;
            }
            '\\' => match chars.clone().next() {
                Some(next @ ('$' | '`' | '"' | '\\')) => {
                    path.push(next.to_string());
                    chars.next();
                }
                _ => path.push("\\"),
            },
            other => path.push(other.to_string()),
        }
    }
    let trailing = chars.as_str().trim();
    let path = PathBuf::from(path);
    (closed && (trailing.is_empty() || trailing.starts_with('#')) && path.is_absolute())
        .then_some(path)
}

pub(super) fn profile_directory(application_dir: &Path, explicit: Option<&Path>) -> PathBuf {
    explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| application_dir.join("profile"))
}

#[derive(Clone, Copy)]
pub(super) enum Platform {
    #[cfg(any(target_os = "macos", test))]
    MacOs,
    #[cfg(any(not(target_os = "macos"), test))]
    Linux,
}

pub(super) fn application_dir(
    platform: Platform,
    home: Option<&Path>,
    xdg_data_home: Option<&Path>,
    development: bool,
) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let _ = xdg_data_home;
    let root = match platform {
        #[cfg(any(target_os = "macos", test))]
        Platform::MacOs => home?.join("Library/Application Support/Elaris"),
        #[cfg(any(not(target_os = "macos"), test))]
        Platform::Linux => {
            let data_home = xdg_data_home
                .filter(|path| path.is_absolute())
                .map(Path::to_path_buf)
                .or_else(|| home.map(|home| home.join(".local/share")))?;
            data_home.join("Elaris")
        }
    };
    Some(if development {
        root.join("development")
    } else {
        root
    })
}

pub(super) fn config_directory(
    platform: Platform,
    home: Option<&Path>,
    _xdg_config_home: Option<&Path>,
    development: bool,
) -> Option<PathBuf> {
    let root = match platform {
        #[cfg(any(target_os = "macos", test))]
        Platform::MacOs => home?.join("Library/Application Support/Elaris"),
        #[cfg(any(not(target_os = "macos"), test))]
        Platform::Linux => _xdg_config_home
            .filter(|path| path.is_absolute())
            .map(Path::to_path_buf)
            .or_else(|| home.map(|home| home.join(".config")))?
            .join("elaris"),
    };
    Some(if development {
        root.join("development")
    } else {
        root
    })
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}
