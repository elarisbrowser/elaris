// SPDX-License-Identifier: GPL-3.0-only

//! Disk use for the persistent web profile and Elaris browsing history.

use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn measure(profile_dir: &Path, history_file: &Path, downloads_file: &Path) -> u64 {
    let profile_size = path_size(profile_dir);
    let history_size = [
        history_file.to_path_buf(),
        suffixed_path(history_file, "-wal"),
        suffixed_path(history_file, "-shm"),
    ]
    .into_iter()
    .filter(|path| !path.starts_with(profile_dir))
    .map(|path| path_size(&path))
    .fold(0, u64::saturating_add);
    let downloads_size = if downloads_file.starts_with(profile_dir) {
        0
    } else {
        path_size(downloads_file)
    };

    profile_size
        .saturating_add(history_size)
        .saturating_add(downloads_size)
}

fn path_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    let file_type = metadata.file_type();
    if file_type.is_file() {
        return metadata.len();
    }
    if !file_type.is_dir() {
        return 0;
    }

    fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| path_size(&entry.path()))
        .fold(0, u64::saturating_add)
}

fn suffixed_path(path: &Path, suffix: &str) -> PathBuf {
    let mut suffixed = path.as_os_str().to_os_string();
    suffixed.push(suffix);
    PathBuf::from(suffixed)
}

#[cfg(test)]
mod tests {
    use super::{measure, suffixed_path};
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "elaris-browsing-data-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove test directory");
        }
    }

    fn write_bytes(path: &std::path::Path, length: usize) {
        let mut file = fs::File::create(path).expect("create test file");
        file.write_all(&vec![0; length]).expect("write test file");
    }

    #[test]
    fn measures_profile_history_and_sqlite_sidecars() {
        let root = TestDirectory::new();
        let profile = root.0.join("profile");
        fs::create_dir_all(profile.join("nested")).expect("create profile directories");
        write_bytes(&profile.join("cookies.sqlite"), 100);
        write_bytes(&profile.join("nested/site-data"), 25);
        let history = root.0.join("history.sqlite");
        write_bytes(&history, 30);
        write_bytes(&suffixed_path(&history, "-wal"), 7);
        write_bytes(&suffixed_path(&history, "-shm"), 3);
        let downloads = root.0.join("downloads.json");
        write_bytes(&downloads, 12);

        assert_eq!(measure(&profile, &history, &downloads), 177);
    }

    #[test]
    fn does_not_count_history_twice_when_the_profile_contains_it() {
        let root = TestDirectory::new();
        let profile = root.0.join("profile");
        fs::create_dir_all(&profile).expect("create profile directory");
        let history = profile.join("history.sqlite");
        write_bytes(&history, 30);
        write_bytes(&suffixed_path(&history, "-wal"), 7);
        let downloads = profile.join("downloads.json");
        write_bytes(&downloads, 12);

        assert_eq!(measure(&profile, &history, &downloads), 49);
    }
}
