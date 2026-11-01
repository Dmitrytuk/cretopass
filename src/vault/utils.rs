use std::fmt;
use std::path::{Path, PathBuf};

use rkyv::{Archive, Deserialize, Serialize, rancor};

use crate::file_handling;

const CONFIG_FILE_NAME: &str = "config.pwd";

/// Error produced by configuration persistence operations.
#[derive(Debug)]
pub enum ConfigError {
    Io(crate::file_handling::FileHandlingError),
    Serialize(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Serialize(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serialize(_) => None,
        }
    }
}

/// Persistent app configuration stored in `~/.config/cretopass/config.pwd`.
#[derive(Debug, Clone, Archive, Deserialize, Serialize)]
pub struct Configuration {
    /// Recently opened secret files (most recent first).
    pub recent_files: Vec<String>,
    /// The last file the user opened or created.
    pub last_opened_file: Option<String>,
    /// If `true`, window title shows the full path; otherwise just the filename.
    pub display_full_path: bool,
    /// If `true`, automatically open the last-opened file on startup.
    pub open_last_opened_file_on_startup: bool,
    /// Canonical name of the active colour scheme (see
    /// [`theming::SCHEME_DARK`](crate::theming::SCHEME_DARK) and
    /// [`theming::SCHEME_LIGHT`](crate::theming::SCHEME_LIGHT)).
    pub palette_name: String,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            recent_files: Vec::new(),
            last_opened_file: None,
            display_full_path: true,
            open_last_opened_file_on_startup: true,
            palette_name: crate::theming::SCHEME_DARK.to_string(),
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
/// In debug builds the `CRETOPASS_CONFIG_DIR` environment variable overrides
/// the location (primarily for tests). The override is compiled out of release
/// builds so a set-and-forget environment variable can never redirect the
/// destructive `cretopass remove` path at an arbitrary directory.
pub fn config_dir() -> PathBuf {
    #[cfg(debug_assertions)]
    let (config_dir_override, environment) = (
        std::env::var("CRETOPASS_CONFIG_DIR").ok(),
        std::env::var("CRETOPASS_ENV").ok(),
    );
    #[cfg(not(debug_assertions))]
    let (config_dir_override, environment) = (None, None);

    config_dir_from(
        config_dir_override,
        environment,
        std::env::var("XDG_CONFIG_HOME").ok(),
        std::env::var("HOME").ok(),
        std::env::var("APPDATA").ok(),
    )
}

/// Resolves the config directory from already-read environment values.
///
/// Kept a pure function so the tests can exercise every branch — including the
/// debug-only overrides — without mutating the process-global environment, which
/// is undefined behaviour under the default parallel test runner.
fn config_dir_from(
    config_dir_override: Option<String>,
    environment: Option<String>,
    xdg_config_home: Option<String>,
    home: Option<String>,
    appdata: Option<String>,
) -> PathBuf {
    #[cfg(debug_assertions)]
    {
        if let Some(val) = config_dir_override.as_deref()
            && !val.is_empty()
        {
            return PathBuf::from(val);
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = config_dir_override;

    #[cfg(debug_assertions)]
    {
        if environment.as_deref() == Some("development") {
            return PathBuf::from("config_dev");
        }
    }
    #[cfg(not(debug_assertions))]
    let _ = environment;

    if cfg!(target_os = "linux") {
        if let Some(val) = xdg_config_home {
            let mut p = PathBuf::from(val);
            p.push("cretopass");
            return p;
        }
        if let Some(val) = home {
            let mut p = PathBuf::from(val);
            p.push(".config/cretopass");
            return p;
        }
    } else if cfg!(target_os = "macos") {
        if let Some(val) = home {
            let mut p = PathBuf::from(val);
            p.push("Library/Application Support/cretopass");
            return p;
        }
    } else if cfg!(target_os = "windows")
        && let Some(val) = appdata
    {
        let mut p = PathBuf::from(val);
        p.push("cretopass");
        return p;
    }
    let mut p = PathBuf::from(".");
    p.push(".cretopass");
    p
}

/// Returns the configuration file path for a given config directory.
///
/// Migrates from the old `settings.pwd` filename if it still exists.
fn config_file_path(dir: &Path) -> PathBuf {
    let new_path = dir.join(CONFIG_FILE_NAME);
    let old_path = dir.join("settings.pwd");
    if old_path.exists() && !new_path.exists() {
        let _ = file_handling::rename_file(&old_path, &new_path);
    }
    new_path
}

/// Saves app configuration to the given config directory as binary.
fn save_config_to(dir: &Path, config: &Configuration) -> Result<(), ConfigError> {
    let path = config_file_path(dir);
    file_handling::create_private_dir(dir).map_err(ConfigError::Io)?;
    let bytes = rkyv::to_bytes::<rancor::Error>(config)
        .map_err(|e| ConfigError::Serialize(e.to_string()))?;
    file_handling::write_file(&path.to_string_lossy(), &bytes).map_err(ConfigError::Io)?;
    Ok(())
}

/// Loads app configuration from the given config directory, falling back to defaults.
fn load_config_from(dir: &Path) -> Configuration {
    let path = config_file_path(dir);

    let bytes = match file_handling::read_file(&path.to_string_lossy()) {
        Ok(b) => b,
        Err(_) => {
            let defaults = Configuration::default();
            let _ = save_config_to(dir, &defaults);
            return defaults;
        }
    };

    let archived = match rkyv::access::<ArchivedConfiguration, rancor::Error>(&bytes) {
        Ok(a) => a,
        Err(_) => {
            let defaults = Configuration::default();
            let _ = save_config_to(dir, &defaults);
            return defaults;
        }
    };

    match rkyv::deserialize::<Configuration, rancor::Error>(archived) {
        Ok(s) => s,
        Err(_) => {
            let defaults = Configuration::default();
            let _ = save_config_to(dir, &defaults);
            defaults
        }
    }
}

/// Saves app configuration to `~/.config/cretopass/config.pwd` as binary.
pub fn save_config(config: &Configuration) -> Result<(), ConfigError> {
    save_config_to(&config_dir(), config)
}

/// Loads app configuration from the config directory.
///
/// If the file does not exist, cannot be opened (for example a symlinked
/// `config.pwd`, which the gateway's symlink-safe read refuses and then
/// replaces), is corrupt, or has an incompatible format (e.g. from an older
/// version), it is recreated with default values.
pub fn load_config() -> Configuration {
    load_config_from(&config_dir())
}

/// Inserts or moves `path` to the front of the recent-files list and saves.
///
/// Duplicates are removed first.
pub fn push_recent_secret_file(config: &mut Configuration, path: &str) -> Result<(), ConfigError> {
    push_recent_secret_file_to(&config_dir(), config, path)
}

/// [`push_recent_secret_file`] against an explicit directory, so tests can run
/// without touching the process environment.
fn push_recent_secret_file_to(
    dir: &Path,
    config: &mut Configuration,
    path: &str,
) -> Result<(), ConfigError> {
    config.recent_files.retain(|f| f != path);
    config.recent_files.insert(0, path.to_string());
    config.last_opened_file = Some(path.to_string());
    save_config_to(dir, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp directory standing in for the config directory, so a test never
    /// touches the developer's real `~/.config/cretopass`.
    fn temp_config_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("a temp config directory")
    }

    #[test]
    fn test_config_save_load() {
        let dir = temp_config_dir();

        let config = Configuration {
            recent_files: vec![
                "/home/user/vault.age".to_string(),
                "/home/user/backup.age".to_string(),
            ],
            last_opened_file: Some("/home/user/vault.age".to_string()),
            display_full_path: false,
            open_last_opened_file_on_startup: true,
            palette_name: crate::theming::SCHEME_LIGHT.to_string(),
        };
        save_config_to(dir.path(), &config).unwrap();
        let loaded = load_config_from(dir.path());

        assert_eq!(loaded.recent_files.len(), 2);
        assert_eq!(loaded.recent_files[0], "/home/user/vault.age");
        assert_eq!(loaded.recent_files[1], "/home/user/backup.age");
        assert_eq!(
            loaded.last_opened_file,
            Some("/home/user/vault.age".to_string())
        );
        assert!(!loaded.display_full_path);
        assert_eq!(loaded.palette_name, crate::theming::SCHEME_LIGHT);
    }

    #[test]
    fn test_config_defaults() {
        let config = Configuration::default();
        assert!(config.display_full_path);
        assert!(config.open_last_opened_file_on_startup);
        assert!(config.recent_files.is_empty());
        assert!(config.last_opened_file.is_none());
        assert_eq!(config.palette_name, crate::theming::SCHEME_DARK);
    }

    #[test]
    fn test_config_save_empty() {
        let dir = temp_config_dir();

        save_config_to(dir.path(), &Configuration::default()).unwrap();
        let loaded = load_config_from(dir.path());
        assert!(loaded.recent_files.is_empty());
        assert!(loaded.last_opened_file.is_none());
        assert!(loaded.display_full_path);
        assert!(loaded.open_last_opened_file_on_startup);
    }

    #[test]
    fn test_config_no_file_creates_defaults() {
        let dir = temp_config_dir();

        let loaded = load_config_from(dir.path());

        assert!(loaded.recent_files.is_empty());
        assert!(loaded.display_full_path);
        assert!(loaded.open_last_opened_file_on_startup);

        assert!(dir.path().join(CONFIG_FILE_NAME).exists());
    }

    /// A symlinked `config.pwd` is not followed: the gateway's `O_NOFOLLOW`
    /// read refuses it, defaults are substituted, and the write-back replaces
    /// the link with a regular file (see ADR 0001 / ADR 0006).
    #[test]
    #[cfg(unix)]
    fn test_config_behind_a_symlink_is_refused_and_replaced() {
        use std::os::unix::fs::symlink;

        let dir = temp_config_dir();
        let elsewhere = temp_config_dir();

        // A real config with non-default content, reached through a symlink.
        let target_config = Configuration {
            recent_files: vec!["/secret/vault.age".to_string()],
            ..Configuration::default()
        };
        save_config_to(elsewhere.path(), &target_config).unwrap();
        let target = elsewhere.path().join(CONFIG_FILE_NAME);
        let link = dir.path().join(CONFIG_FILE_NAME);
        symlink(&target, &link).unwrap();

        let loaded = load_config_from(dir.path());

        // The symlinked content is not read: defaults come back instead.
        assert!(
            loaded.recent_files.is_empty(),
            "a symlinked config must not load: {:?}",
            loaded.recent_files
        );
        // The failed load writes defaults back over the link, replacing it.
        assert!(
            !std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink(),
            "the write-back must replace the symlink with a regular file"
        );
        // The original target is untouched.
        let still = load_config_from(elsewhere.path());
        assert_eq!(still.recent_files, vec!["/secret/vault.age".to_string()]);
    }

    #[test]
    fn test_push_recent_secret_file_dedup() {
        let dir = temp_config_dir();

        let mut config = Configuration {
            recent_files: vec![
                "/alpha.age".to_string(),
                "/beta.age".to_string(),
                "/gamma.age".to_string(),
            ],
            last_opened_file: None,
            display_full_path: true,
            open_last_opened_file_on_startup: true,
            palette_name: crate::theming::SCHEME_DARK.to_string(),
        };
        push_recent_secret_file_to(dir.path(), &mut config, "/beta.age").unwrap();

        assert_eq!(config.recent_files.len(), 3);
        assert_eq!(config.recent_files[0], "/beta.age");
        assert_eq!(config.recent_files[1], "/alpha.age");
        assert_eq!(config.recent_files[2], "/gamma.age");
        assert_eq!(config.last_opened_file, Some("/beta.age".to_string()));
    }

    #[test]
    fn test_push_recent_secret_file_sets_last_opened() {
        let dir = temp_config_dir();

        let mut config = Configuration::default();
        push_recent_secret_file_to(dir.path(), &mut config, "/new.age").unwrap();

        assert_eq!(config.recent_files.len(), 1);
        assert_eq!(config.recent_files[0], "/new.age");
        assert_eq!(config.last_opened_file, Some("/new.age".to_string()));
    }

    #[test]
    fn test_config_roundtrip_display_full_path_false() {
        let dir = temp_config_dir();

        let config = Configuration {
            recent_files: vec!["/a.age".to_string()],
            display_full_path: false,
            open_last_opened_file_on_startup: false,
            ..Default::default()
        };
        save_config_to(dir.path(), &config).unwrap();

        let loaded = load_config_from(dir.path());
        assert!(!loaded.display_full_path);
        assert!(!loaded.open_last_opened_file_on_startup);
    }

    // ── `config_dir_from`: every environment branch, tested purely ──

    #[cfg(debug_assertions)]
    #[test]
    fn config_dir_uses_the_debug_override_when_set() {
        let dir = config_dir_from(
            Some("/custom/config".to_string()),
            Some("development".to_string()),
            None,
            None,
            None,
        );
        assert_eq!(dir, PathBuf::from("/custom/config"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn config_dir_ignores_an_empty_override() {
        let dir = config_dir_from(
            Some(String::new()),
            Some("development".to_string()),
            None,
            None,
            None,
        );
        assert_eq!(dir, PathBuf::from("config_dev"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn config_dir_dev_mode_selects_the_in_tree_directory() {
        let dir = config_dir_from(None, Some("development".to_string()), None, None, None);
        assert_eq!(dir, PathBuf::from("config_dev"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn config_dir_dev_mode_falls_through_on_other_values() {
        let dir = config_dir_from(None, Some("production".to_string()), None, None, None);
        assert!(!dir.ends_with("config_dev"));
    }

    #[test]
    fn config_dir_falls_back_to_the_dot_directory_without_any_env() {
        assert_eq!(
            config_dir_from(None, None, None, None, None),
            PathBuf::from("./.cretopass")
        );
    }
}
