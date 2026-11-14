//! Configuration-directory resolution.
//!
//! A strict leaf: it knows only where cretopass keeps its configuration, not
//! what is stored there. The Home screen's [`Configuration`](crate::vault::utils::Configuration)
//! persistence and the lifecycle `remove` command both consume it, so the
//! destructive path never depends on the Home-screen module.

use std::path::PathBuf;

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

#[cfg(test)]
mod tests {
    use super::*;

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
