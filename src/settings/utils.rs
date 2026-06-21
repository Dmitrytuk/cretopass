use std::path::PathBuf;

use rkyv::{rancor, Archive, Deserialize, Serialize};

use crate::file_handling;

const SETTINGS_FILE_NAME: &str = "settings.pwd";

/// Persistent app settings stored in `~/.config/pwd-keeper/settings.pwd`.
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

/// Returns the platform-specific config directory for pwd-keeper.
///
/// - Linux: `$XDG_CONFIG_HOME/pwd-keeper/` or `~/.config/pwd-keeper/`
/// - macOS: `~/Library/Application Support/pwd-keeper/`
/// - Windows: `%APPDATA%/pwd-keeper/`
/// - Fallback: `./.pwd-keeper/`
fn config_dir() -> PathBuf {
    if cfg!(target_os = "linux") {
        if let Ok(val) = std::env::var("XDG_CONFIG_HOME") {
            let mut p = PathBuf::from(val);
            p.push("pwd-keeper");
            return p;
        }
        if let Ok(val) = std::env::var("HOME") {
            let mut p = PathBuf::from(val);
            p.push(".config/pwd-keeper");
            return p;
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(val) = std::env::var("HOME") {
            let mut p = PathBuf::from(val);
            p.push("Library/Application Support/pwd-keeper");
            return p;
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(val) = std::env::var("APPDATA") {
            let mut p = PathBuf::from(val);
            p.push("pwd-keeper");
            return p;
        }
    }
    let mut p = PathBuf::from(".");
    p.push(".pwd-keeper");
    p
}

/// Saves app settings to `~/.config/pwd-keeper/settings.pwd` as binary.
pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let dir = config_dir();
    let path = dir.join(SETTINGS_FILE_NAME);
    file_handling::create_dir(&dir).map_err(|e| format!("Cannot create config directory: {e}"))?;
    let bytes = rkyv::to_bytes::<rancor::Error>(settings)
        .map_err(|e| format!("Failed to serialise settings: {e}"))?;
    file_handling::write_file(
        &path.to_string_lossy(),
        &bytes,
    )
    .map_err(|e| format!("Failed to write settings: {e}"))?;
    Ok(())
}

/// Returns default settings and persists them to disk.
fn create_default_settings() -> AppSettings {
    let defaults = AppSettings::default();
    let _ = save_settings(&defaults);
    defaults
}

/// Loads app settings from the config directory.
///
/// If the file does not exist, is corrupt, or has an incompatible format
/// (e.g. from an older version), it is recreated with default values.
pub fn load_settings() -> AppSettings {
    let path = config_dir().join(SETTINGS_FILE_NAME);

    let bytes = match file_handling::read_file(&path.to_string_lossy()) {
        Ok(b) => b,
        Err(_) => return create_default_settings(),
    };

    let archived = match rkyv::access::<ArchivedAppSettings, rancor::Error>(&bytes) {
        Ok(a) => a,
        Err(_) => return create_default_settings(),
    };

    match rkyv::deserialize::<AppSettings, rancor::Error>(archived) {
        Ok(s) => s,
        Err(_) => create_default_settings(),
    }
}

/// Attempts to open an OS-native file-selection dialog.
///
/// Tries, in order:
/// - Linux: `zenity --file-selection`
/// - Linux: `kdialog --getopenfilename`
/// - macOS: `osascript -e 'POSIX path of (choose file)'`
/// - Windows: `powershell -Command ...`
///
/// Returns `None` if no dialog tool is available or the user cancels.
pub fn open_file_dialog() -> Option<String> {
    if cfg!(target_os = "linux") {
        let result = std::process::Command::new("zenity")
            .args(["--file-selection"])
            .output();
        if let Ok(output) = result {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }

        let result = std::process::Command::new("kdialog")
            .args(["--getopenfilename", "."])
            .output();
        if let Ok(output) = result {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        let result = std::process::Command::new("osascript")
            .args(["-e", "POSIX path of (choose file)"])
            .output();
        if let Ok(output) = result {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    } else if cfg!(target_os = "windows") {
        let result = std::process::Command::new("powershell")
            .args([
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $f = New-Object System.Windows.Forms.OpenFileDialog; \
                 $f.ShowDialog() | Out-Null; \
                 Write-Host $f.FileName",
            ])
            .output();
        if let Ok(output) = result {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    None
}

/// Inserts or moves `path` to the front of the recent-files list and saves.
/// Duplicates are removed first.
pub fn push_recent_file(settings: &mut AppSettings, path: &str) -> Result<(), String> {
    settings.recent_files.retain(|f| f != path);
    settings.recent_files.insert(0, path.to_string());
    settings.last_opened_file = Some(path.to_string());
    save_settings(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clean_settings_file() {
        let path = config_dir().join(SETTINGS_FILE_NAME);
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
    }

    #[test]
    fn test_settings_save_load() {
        clean_settings_file();

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
        assert_eq!(loaded.last_opened_file, Some("/home/user/vault.age".to_string()));
        assert!(!loaded.display_full_path);

        clean_settings_file();
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
        clean_settings_file();

        let settings = AppSettings::default();
        save_settings(&settings).unwrap();
        let loaded = load_settings();
        assert!(loaded.recent_files.is_empty());
        assert!(loaded.last_opened_file.is_none());
        assert!(loaded.display_full_path);
        assert!(loaded.open_last_opened_file_on_startup);

        clean_settings_file();
    }

    #[test]
    fn test_settings_no_file_creates_defaults() {
        clean_settings_file();

        let loaded = load_settings();

        assert!(loaded.recent_files.is_empty());
        assert!(loaded.display_full_path);
        assert!(loaded.open_last_opened_file_on_startup);

        let path = config_dir().join(SETTINGS_FILE_NAME);
        assert!(path.exists());

        clean_settings_file();
    }

    #[test]
    fn test_push_recent_file_dedup() {
        clean_settings_file();

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
        assert_eq!(
            settings.last_opened_file,
            Some("/beta.age".to_string())
        );

        clean_settings_file();
    }

    #[test]
    fn test_push_recent_file_sets_last_opened() {
        clean_settings_file();

        let mut settings = AppSettings::default();
        push_recent_file(&mut settings, "/new.age").unwrap();

        assert_eq!(settings.recent_files.len(), 1);
        assert_eq!(settings.recent_files[0], "/new.age");
        assert_eq!(settings.last_opened_file, Some("/new.age".to_string()));

        clean_settings_file();
    }

    #[test]
    fn test_settings_roundtrip_display_full_path_false() {
        clean_settings_file();

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

        clean_settings_file();
    }
}
