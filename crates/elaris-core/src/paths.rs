// SPDX-License-Identifier: GPL-3.0-only

//! Where Elaris keeps its data and config on each platform, and how paths appear in messages.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paths {
    pub application_dir: PathBuf,
    pub profile_dir: PathBuf,
    pub history_file: PathBuf,
    pub session_file: PathBuf,
    /// Finished downloads kept across restarts (`download_history`).
    pub download_history_file: PathBuf,
    pub config_file: PathBuf,
}

impl Paths {
    pub fn resolve() -> io::Result<Self> {
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

    pub fn resolve_with(
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
            download_history_file: application_dir.join("downloads.json"),
            config_file: config_dir.join("config.toml"),
            application_dir,
            profile_dir,
        })
    }

    pub fn create_directories(&self) -> io::Result<()> {
        create_private_dir(&self.application_dir)?;
        create_private_dir(&self.profile_dir)?;
        if let Some(parent) = self.config_file.parent() {
            create_private_dir(parent)?;
        }
        Ok(())
    }
}

/// A path for a message. The home directory shows as `~`, so logs and errors do not carry the user's name.
pub fn display_path(path: &Path) -> String {
    redact_home(path, std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

fn redact_home(path: &Path, home: Option<&Path>) -> String {
    if let Some(home) = home
        && let Ok(rest) = path.strip_prefix(home)
    {
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

#[cfg(test)]
mod redaction_tests {
    use super::redact_home;
    use std::path::Path;

    #[test]
    fn the_home_directory_shows_as_tilde() {
        let home = Path::new("/Users/someone");
        assert_eq!(
            redact_home(
                Path::new("/Users/someone/Library/Application Support/Elaris/session.json"),
                Some(home)
            ),
            "~/Library/Application Support/Elaris/session.json"
        );
        assert_eq!(
            redact_home(Path::new("/var/tmp/profile"), Some(home)),
            "/var/tmp/profile"
        );
        assert_eq!(
            redact_home(Path::new("/Users/someonelse/x"), Some(home)),
            "/Users/someonelse/x"
        );
        assert_eq!(
            redact_home(Path::new("/Users/someone/x"), None),
            "/Users/someone/x"
        );
    }
}

pub fn default_download_directory() -> PathBuf {
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

pub fn parse_xdg_download_directory(line: &str, home: &Path) -> Option<PathBuf> {
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

pub fn profile_directory(application_dir: &Path, explicit: Option<&Path>) -> PathBuf {
    explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| application_dir.join("profile"))
}

#[derive(Clone, Copy)]
/// The platform whose conventions pick the directories. Both are always available, so the path rules are tested
/// on every host.
pub enum Platform {
    MacOs,
    Linux,
}

pub fn application_dir(
    platform: Platform,
    home: Option<&Path>,
    xdg_data_home: Option<&Path>,
    development: bool,
) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    let _ = xdg_data_home;
    let root = match platform {
        Platform::MacOs => home?.join("Library/Application Support/Elaris"),
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

pub fn config_directory(
    platform: Platform,
    home: Option<&Path>,
    _xdg_config_home: Option<&Path>,
    development: bool,
) -> Option<PathBuf> {
    let root = match platform {
        Platform::MacOs => home?.join("Library/Application Support/Elaris"),
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

#[cfg(test)]
mod resolution_tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("elaris-paths-{}-{id}", std::process::id()));
            fs::create_dir_all(&path).expect("create temporary data directory");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn macos_and_linux_paths_follow_native_conventions() {
        let home = Path::new("/home/tester");
        assert_eq!(
            application_dir(Platform::MacOs, Some(home), None, false),
            Some(home.join("Library/Application Support/Elaris"))
        );
        assert_eq!(
            application_dir(Platform::Linux, Some(home), None, false),
            Some(home.join(".local/share/Elaris"))
        );
        assert_eq!(
            application_dir(
                Platform::Linux,
                Some(home),
                Some(Path::new("/mnt/user-data")),
                false
            ),
            Some(PathBuf::from("/mnt/user-data/Elaris"))
        );
        assert_eq!(
            application_dir(
                Platform::Linux,
                Some(home),
                Some(Path::new("relative")),
                false
            ),
            Some(home.join(".local/share/Elaris"))
        );
    }

    #[test]
    fn development_paths_are_isolated_from_release_paths() {
        let home = Path::new("/home/tester");
        let release = application_dir(Platform::Linux, Some(home), None, false).unwrap();
        let development = application_dir(Platform::Linux, Some(home), None, true).unwrap();
        assert_eq!(development, release.join("development"));
        let config_release = config_directory(Platform::Linux, Some(home), None, false).unwrap();
        let config_development = config_directory(Platform::Linux, Some(home), None, true).unwrap();
        assert_eq!(config_development, config_release.join("development"));
    }

    #[test]
    fn config_paths_follow_platform_conventions_and_xdg() {
        let home = Path::new("/home/tester");
        assert_eq!(
            config_directory(Platform::MacOs, Some(home), None, false),
            Some(home.join("Library/Application Support/Elaris"))
        );
        assert_eq!(
            config_directory(Platform::Linux, Some(home), None, false),
            Some(home.join(".config/elaris"))
        );
        assert_eq!(
            config_directory(
                Platform::Linux,
                Some(home),
                Some(Path::new("/mnt/user-config")),
                false
            ),
            Some(PathBuf::from("/mnt/user-config/elaris"))
        );
        let paths = Paths::resolve_with(
            Platform::Linux,
            None,
            Some(home),
            None,
            Some(Path::new("/mnt/user-config")),
            None,
            false,
        )
        .expect("resolve Linux paths");
        assert_eq!(
            paths.config_file,
            PathBuf::from("/mnt/user-config/elaris/config.toml")
        );
    }

    #[test]
    fn parses_xdg_download_directory_without_evaluating_shell_content() {
        let home = Path::new("/home/tester");
        assert_eq!(
            parse_xdg_download_directory(
                "XDG_DOWNLOAD_DIR=\"$HOME/Shared Downloads\" # user choice",
                home
            ),
            Some(home.join("Shared Downloads"))
        );
        assert_eq!(
            parse_xdg_download_directory("XDG_DOWNLOAD_DIR=\"/mnt/files\"", home),
            Some(PathBuf::from("/mnt/files"))
        );
        assert_eq!(
            parse_xdg_download_directory("XDG_DOWNLOAD_DIR=\"$(touch /tmp/pwned)\"", home),
            None
        );
    }

    #[test]
    fn data_directory_override_is_exact_and_sets_the_default_profile() {
        let temp = TempDir::new();
        let data_dir = temp.0.join("isolated-data");
        let paths = Paths::resolve_with(
            Platform::MacOs,
            Some(&data_dir),
            Some(Path::new("/Users/tester")),
            None,
            None,
            None,
            true,
        )
        .expect("resolve explicit data directory");

        assert_eq!(paths.application_dir, data_dir);
        assert_eq!(paths.profile_dir, paths.application_dir.join("profile"));
        assert_eq!(
            paths.history_file,
            paths.application_dir.join("history.sqlite")
        );
        assert_eq!(
            paths.session_file,
            paths.application_dir.join("session.json")
        );
        assert_eq!(paths.config_file, paths.application_dir.join("config.toml"));
        assert!(!paths.application_dir.exists());
    }

    #[test]
    fn data_directory_override_keeps_the_ladybird_profile_override() {
        let temp = TempDir::new();
        let data_dir = temp.0.join("isolated-data");
        let explicit_profile = temp.0.join("ladybird-profile");
        let paths = Paths::resolve_with(
            Platform::Linux,
            Some(&data_dir),
            Some(Path::new("/home/tester")),
            Some(Path::new("/xdg-data")),
            None,
            Some(&explicit_profile),
            true,
        )
        .expect("resolve explicit paths");

        assert_eq!(paths.application_dir, data_dir);
        assert_eq!(paths.profile_dir, explicit_profile);
        assert!(!paths.application_dir.exists());
        assert!(!paths.profile_dir.exists());
    }

    #[test]
    fn relative_data_directory_overrides_are_rejected() {
        let error = Paths::resolve_with(
            Platform::Linux,
            Some(Path::new("relative-data")),
            Some(Path::new("/home/tester")),
            None,
            None,
            None,
            true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn explicit_ladybird_profile_directory_is_preserved() {
        let application = Path::new("/home/tester/.local/share/Elaris/development");
        let explicit = Path::new("/tmp/elaris-profile-override");
        assert_eq!(profile_directory(application, Some(explicit)), explicit);
        assert_eq!(
            profile_directory(application, None),
            application.join("profile")
        );
    }
}
