use std::fmt;
use std::path::{Path, PathBuf};

use rkyv::{rancor, Archive, Deserialize, Serialize};

use crate::file_handling;

const SETTINGS_FILE_NAME: &str = "settings.pwd";

/// Error produced by settings persistence operations.
#[derive(Debug)]
pub enum SettingsError {
    Io(crate::file_handling::FileHandlingError),
    Serialize(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Serialize(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for SettingsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serialize(_) => None,
        }
    }
}

/// Persistent app settings stored in `~/.config/cretopass/settings.pwd`.
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
pub struct AppSettings {
    /// Recently opened secret files (most recent first).
    pub recent_files: Vec<String>,
    /// The last file the user opened or created.
    pub last_opened_file: Option<String>,
    /// If `true`, window title shows the full path; otherwise just the filename.
    pub display_full_path: bool,
    /// If `true`, automatically open the last-opened file on startup.
    pub open_last_opened_file_on_startup: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            recent_files: Vec::new(),
            last_opened_file: None,
            display_full_path: true,
            open_last_opened_file_on_startup: true,
        }
    }
}

/// Returns the platform-specific config directory for cretopass.
///
/// - Linux: `$XDG_CONFIG_HOME/cretopass/` or `~/.config/cretopass/`
/// - macOS: `~/Library/Application Support/cretopass/`
/// - Windows: `%APPDATA%/cretopass/`
/// - Fallback: `./.cretopass/`
///
/// When the `CRETOPASS_CONFIG_DIR` environment variable is set, its value is
/// used instead (primarily for tests).
pub fn config_dir() -> PathBuf {
    if let Ok(val) = std::env::var("CRETOPASS_CONFIG_DIR") {
        if !val.is_empty() {
            return PathBuf::from(val);
        }
    }
    if cfg!(target_os = "linux") {
        if let Ok(val) = std::env::var("XDG_CONFIG_HOME") {
            let mut p = PathBuf::from(val);
            p.push("cretopass");
            return p;
        }
        if let Ok(val) = std::env::var("HOME") {
            let mut p = PathBuf::from(val);
            p.push(".config/cretopass");
            return p;
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(val) = std::env::var("HOME") {
            let mut p = PathBuf::from(val);
            p.push("Library/Application Support/cretopass");
            return p;
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(val) = std::env::var("APPDATA") {
            let mut p = PathBuf::from(val);
            p.push("cretopass");
            return p;
        }
    }
    let mut p = PathBuf::from(".");
    p.push(".cretopass");
    p
}

/// Returns the settings file path for a given config directory.
fn settings_file_path(dir: &Path) -> PathBuf {
    dir.join(SETTINGS_FILE_NAME)
}

/// Saves app settings to the given config directory as binary.
fn save_settings_to(dir: &Path, settings: &AppSettings) -> Result<(), SettingsError> {
    let path = settings_file_path(dir);
    file_handling::create_dir(dir).map_err(SettingsError::Io)?;
    let bytes = rkyv::to_bytes::<rancor::Error>(settings)
        .map_err(|e| SettingsError::Serialize(e.to_string()))?;
    file_handling::write_file(&path.to_string_lossy(), &bytes).map_err(SettingsError::Io)?;
    Ok(())
}

/// Loads app settings from the given config directory, falling back to defaults.
fn load_settings_from(dir: &Path) -> AppSettings {
    let path = settings_file_path(dir);

    let bytes = match file_handling::read_file(&path.to_string_lossy()) {
        Ok(b) => b,
        Err(_) => {
            let defaults = AppSettings::default();
            let _ = save_settings_to(dir, &defaults);
            return defaults;
        }
    };

    let archived = match rkyv::access::<ArchivedAppSettings, rancor::Error>(&bytes) {
        Ok(a) => a,
        Err(_) => {
            let defaults = AppSettings::default();
            let _ = save_settings_to(dir, &defaults);
            return defaults;
        }
    };

    match rkyv::deserialize::<AppSettings, rancor::Error>(archived) {
        Ok(s) => s,
        Err(_) => {
            let defaults = AppSettings::default();
            let _ = save_settings_to(dir, &defaults);
            defaults
        }
    }
}

/// Saves app settings to `~/.config/cretopass/settings.pwd` as binary.
pub fn save_settings(settings: &AppSettings) -> Result<(), SettingsError> {
    save_settings_to(&config_dir(), settings)
}

/// Loads app settings from the config directory.
///
/// If the file does not exist, is corrupt, or has an incompatible format
/// (e.g. from an older version), it is recreated with default values.
pub fn load_settings() -> AppSettings {
    load_settings_from(&config_dir())
}

/// Attempts to open an OS-native file-selection dialog.
/// Inserts or moves `path` to the front of the recent-files list and saves.
/// Duplicates are removed first.
pub fn push_recent_file(settings: &mut AppSettings, path: &str) -> Result<(), SettingsError> {
    settings.recent_files.retain(|f| f != path);
    settings.recent_files.insert(0, path.to_string());
    settings.last_opened_file = Some(path.to_string());
    save_settings(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points settings I/O at an isolated temp directory so tests never touch
    /// the developer's real `~/.config/cretopass`.
    struct TestConfigDir(PathBuf);

    impl TestConfigDir {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!("cretopass-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap();
            // SAFETY: tests in this module run single-threaded via
            // `cargo test -- --test-threads=1`; no other thread mutates env.
            unsafe { std::env::set_var("CRETOPASS_CONFIG_DIR", &dir) };
            Self(dir)
        }
    }

    impl Drop for TestConfigDir {
        fn drop(&mut self) {
            // SAFETY: see TestConfigDir::new.
            unsafe { std::env::remove_var("CRETOPASS_CONFIG_DIR") };
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_settings_save_load() {
        let _tmp = TestConfigDir::new();

        let settings = AppSettings {
            recent_files: vec![
                "/home/user/vault.age".to_string(),
                "/home/user/backup.age".to_string(),
            ],
            last_opened_file: Some("/home/user/vault.age".to_string()),
            display_full_path: false,
            open_last_opened_file_on_startup: true,
        };
        save_settings(&settings).unwrap();
        let loaded = load_settings();

        assert_eq!(loaded.recent_files.len(), 2);
        assert_eq!(loaded.recent_files[0], "/home/user/vault.age");
        assert_eq!(loaded.recent_files[1], "/home/user/backup.age");
        assert_eq!(
            loaded.last_opened_file,
            Some("/home/user/vault.age".to_string())
        );
        assert!(!loaded.display_full_path);
    }

    #[test]
    fn test_settings_defaults() {
        let settings = AppSettings::default();
        assert!(settings.display_full_path);
        assert!(settings.open_last_opened_file_on_startup);
        assert!(settings.recent_files.is_empty());
        assert!(settings.last_opened_file.is_none());
    }

    #[test]
    fn test_settings_save_empty() {
        let _tmp = TestConfigDir::new();

        let settings = AppSettings::default();
        save_settings(&settings).unwrap();
        let loaded = load_settings();
        assert!(loaded.recent_files.is_empty());
        assert!(loaded.last_opened_file.is_none());
        assert!(loaded.display_full_path);
        assert!(loaded.open_last_opened_file_on_startup);
    }

    #[test]
    fn test_settings_no_file_creates_defaults() {
        let _tmp = TestConfigDir::new();

        let loaded = load_settings();

        assert!(loaded.recent_files.is_empty());
        assert!(loaded.display_full_path);
        assert!(loaded.open_last_opened_file_on_startup);

        let path = config_dir().join(SETTINGS_FILE_NAME);
        assert!(path.exists());
    }

    #[test]
    fn test_push_recent_file_dedup() {
        let _tmp = TestConfigDir::new();

        let mut settings = AppSettings {
            recent_files: vec![
                "/alpha.age".to_string(),
                "/beta.age".to_string(),
                "/gamma.age".to_string(),
            ],
            last_opened_file: None,
            display_full_path: true,
            open_last_opened_file_on_startup: true,
        };
        push_recent_file(&mut settings, "/beta.age").unwrap();

        assert_eq!(settings.recent_files.len(), 3);
        assert_eq!(settings.recent_files[0], "/beta.age");
        assert_eq!(settings.recent_files[1], "/alpha.age");
        assert_eq!(settings.recent_files[2], "/gamma.age");
        assert_eq!(settings.last_opened_file, Some("/beta.age".to_string()));
    }

    #[test]
    fn test_push_recent_file_sets_last_opened() {
        let _tmp = TestConfigDir::new();

        let mut settings = AppSettings::default();
        push_recent_file(&mut settings, "/new.age").unwrap();

        assert_eq!(settings.recent_files.len(), 1);
        assert_eq!(settings.recent_files[0], "/new.age");
        assert_eq!(settings.last_opened_file, Some("/new.age".to_string()));
    }

    #[test]
    fn test_settings_roundtrip_display_full_path_false() {
        let _tmp = TestConfigDir::new();

        let settings = AppSettings {
            recent_files: vec!["/a.age".to_string()],
            display_full_path: false,
            open_last_opened_file_on_startup: false,
            ..Default::default()
        };
        save_settings(&settings).unwrap();

        let loaded = load_settings();
        assert!(!loaded.display_full_path);
        assert!(!loaded.open_last_opened_file_on_startup);
    }
}
