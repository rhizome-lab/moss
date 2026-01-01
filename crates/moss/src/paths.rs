//! Path utilities for moss data directories.
//!
//! Supports external index locations via MOSS_INDEX_DIR environment variable.
//! This allows repos without `.moss` in `.gitignore` to store indexes elsewhere.

use std::path::{Path, PathBuf};

/// Get the moss data directory for a project.
///
/// Resolution order:
/// 1. If MOSS_INDEX_DIR is set to an absolute path, use it directly
/// 2. If MOSS_INDEX_DIR is set to a relative path, use $XDG_DATA_HOME/moss/<relative>
/// 3. Otherwise, use <root>/.moss
///
/// Examples:
/// - MOSS_INDEX_DIR="/tmp/moss-data" -> /tmp/moss-data
/// - MOSS_INDEX_DIR="myproject" -> ~/.local/share/moss/myproject
/// - (unset) -> <root>/.moss
pub fn get_moss_dir(root: &Path) -> PathBuf {
    if let Ok(index_dir) = std::env::var("MOSS_INDEX_DIR") {
        let path = PathBuf::from(&index_dir);
        if path.is_absolute() {
            return path;
        }
        // Relative path: use XDG_DATA_HOME/moss/<relative>
        let data_home = std::env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local/share")
            });
        return data_home.join("moss").join(&index_dir);
    }
    root.join(".moss")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    // SAFETY: These tests run single-threaded via `cargo test -- --test-threads=1`
    // or are independent enough that env var conflicts are unlikely in practice.
    // set_var/remove_var are unsafe in edition 2024 due to potential data races
    // when other threads read the environment concurrently.

    #[test]
    fn test_default_moss_dir() {
        unsafe { env::remove_var("MOSS_INDEX_DIR") };
        let root = PathBuf::from("/project");
        assert_eq!(get_moss_dir(&root), PathBuf::from("/project/.moss"));
    }

    #[test]
    fn test_absolute_moss_index_dir() {
        unsafe { env::set_var("MOSS_INDEX_DIR", "/custom/path") };
        let root = PathBuf::from("/project");
        assert_eq!(get_moss_dir(&root), PathBuf::from("/custom/path"));
        unsafe { env::remove_var("MOSS_INDEX_DIR") };
    }

    #[test]
    fn test_relative_moss_index_dir() {
        unsafe { env::set_var("MOSS_INDEX_DIR", "myproject") };
        unsafe { env::set_var("XDG_DATA_HOME", "/home/user/.data") };
        let root = PathBuf::from("/project");
        assert_eq!(
            get_moss_dir(&root),
            PathBuf::from("/home/user/.data/moss/myproject")
        );
        unsafe { env::remove_var("MOSS_INDEX_DIR") };
        unsafe { env::remove_var("XDG_DATA_HOME") };
    }
}
