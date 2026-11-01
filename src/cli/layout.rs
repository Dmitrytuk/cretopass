//! Single source of truth for the on-disk install layout.
//!
//! `install.sh` / `install.ps1` are the canonical record of what a
//! GitHub-Releases installation creates. This module is the Rust mirror that
//! [`detect`](super::detect) and [`remove`](super::remove) read, so the layout
//! exists in exactly one place on the Rust side (see ADR 0008).
//!
//! It also owns the safe temporary-directory primitive shared by `update` and
//! `remove`, so both commands create `0700` directories with unpredictable
//! names instead of a predictable `{prefix}-{pid}` path.

use std::io;
use std::path::{Path, PathBuf};

/// Name of the installed binary, including the platform executable suffix.
#[cfg(unix)]
pub(crate) const BIN_NAME: &str = "cretopass";
/// Name of the installed binary, including the platform executable suffix.
#[cfg(windows)]
pub(crate) const BIN_NAME: &str = "cretopass.exe";

/// Returns an environment variable as a non-empty [`PathBuf`].
///
/// An unset or empty value is treated as absent so callers never build a
/// path from an empty string (which would resolve relative to the cwd).
pub(crate) fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

/// Returns the user's home directory, or `None` when it is unset or empty.
pub(crate) fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        env_path("HOME")
    }
    #[cfg(windows)]
    {
        env_path("USERPROFILE")
    }
}

/// Directory the binary is installed into.
#[cfg(unix)]
pub(crate) fn bin_dir(home: &Path) -> PathBuf {
    home.join(".local/bin")
}

/// Directory the binary is installed into.
#[cfg(windows)]
pub(crate) fn bin_dir(home: &Path) -> PathBuf {
    install_dir(home)
}

/// The Windows installation directory (`%LOCALAPPDATA%\Programs\cretopass`).
///
/// Falls back to a home-relative path when `LOCALAPPDATA` is unset, so the
/// result is never an empty (cwd-relative) path.
#[cfg(windows)]
pub(crate) fn install_dir(home: &Path) -> PathBuf {
    env_path("LOCALAPPDATA")
        .unwrap_or_else(|| home.join("AppData/Local"))
        .join("Programs")
        .join("cretopass")
}

/// Returns the installed binary path.
pub(crate) fn binary_path(home: &Path) -> PathBuf {
    bin_dir(home).join(BIN_NAME)
}

/// Returns the installed shell-completion files.
pub(crate) fn completions(home: &Path) -> Vec<PathBuf> {
    #[cfg(unix)]
    {
        vec![
            home.join(".local/share/bash-completion/completions/cretopass"),
            home.join(".zsh/completions/_cretopass"),
            home.join(".config/fish/completions/cretopass.fish"),
        ]
    }
    #[cfg(windows)]
    {
        vec![powershell_completion(home)]
    }
}

/// Returns the shell rc files the installer may edit (Unix only).
#[cfg(unix)]
pub(crate) fn rc_files(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".zshrc"),
        home.join(".bashrc"),
        home.join(".config/fish/config.fish"),
    ]
}

/// Returns the GUI payload paths (`cretopass.app` on macOS, `.desktop` +
/// icon on Linux; none on Windows, where the Start Menu shortcut is handled
/// separately).
pub(crate) fn gui_artifacts(home: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        vec![
            home.join(".local/share/applications/cretopass.desktop"),
            home.join(".local/share/icons/hicolor/256x256/apps/cretopass.png"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![home.join("Applications/cretopass.app")]
    }
    #[cfg(windows)]
    {
        let _ = home;
        Vec::new()
    }
}

/// The desktop-entry directory whose cache is refreshed after removal
/// (Linux only).
#[cfg(target_os = "linux")]
pub(crate) fn desktop_dir(home: &Path) -> PathBuf {
    home.join(".local/share/applications")
}

/// The icon theme directory whose cache is refreshed after removal
/// (Linux only).
#[cfg(target_os = "linux")]
pub(crate) fn icon_theme_dir(home: &Path) -> PathBuf {
    home.join(".local/share/icons/hicolor")
}

/// The Start Menu shortcut the installer creates (Windows only).
#[cfg(windows)]
pub(crate) fn start_menu_shortcut() -> Option<PathBuf> {
    env_path("APPDATA").map(|appdata| {
        appdata
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("cretopass.lnk")
    })
}

/// The PowerShell completion file the installer creates (Windows only).
#[cfg(windows)]
pub(crate) fn powershell_completion(home: &Path) -> PathBuf {
    home.join("Documents/PowerShell/Completions/cretopass.ps1")
}

/// The PowerShell profile files that may hold the dot-source line
/// (Windows only).
///
/// `install.ps1` writes to `$PROFILE.CurrentUserAllHosts`, whose location
/// depends on the PowerShell edition (`Documents\PowerShell` for PowerShell 7,
/// `Documents\WindowsPowerShell` for Windows PowerShell 5.1). `remove` edits
/// every candidate so neither edition is left with a dangling dot-source.
#[cfg(windows)]
pub(crate) fn powershell_profiles(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Documents/PowerShell/profile.ps1"),
        home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1"),
        home.join("Documents/WindowsPowerShell/profile.ps1"),
        home.join("Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1"),
    ]
}

/// A uniquely named temporary directory removed on drop.
///
/// The name carries a random UUID so a local attacker cannot predict or
/// pre-create it, it is created with `create_dir` (which fails if the path
/// already exists) rather than `create_dir_all`, and it is created with mode
/// `0700` on Unix.
#[derive(Debug)]
pub(crate) struct TempDir {
    path: PathBuf,
    keep: bool,
}

impl TempDir {
    /// Creates a fresh, private, uniquely named directory under the system
    /// temp directory.
    ///
    /// # Errors
    ///
    /// Propagates the underlying [`io::Error`] when the directory cannot be
    /// created (including when an attacker has pre-created the path).
    pub(crate) fn create(prefix: &str) -> io::Result<Self> {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        create_private_dir(&path)?;
        Ok(Self { path, keep: false })
    }

    /// Returns the directory path.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Leaks the directory so it is not removed on drop.
    ///
    /// Used on Windows, where a detached helper reads the staged script after
    /// the current process has exited.
    // `keep` is only called on the Windows detached-helper path; tests use it
    // too, but non-test Unix builds would otherwise see it as dead code.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) fn keep(mut self) {
        self.keep = true;
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

/// Creates a new file that must not already exist.
///
/// # Errors
///
/// Propagates the underlying [`io::Error`]; in particular fails with
/// [`io::ErrorKind::AlreadyExists`] when the path is occupied.
pub(crate) fn create_new_file(path: &Path) -> io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Rejects an environment-derived path that is unsafe to embed in a generated
/// batch file — either not absolute, or containing a shell metacharacter.
///
/// Kept in this one place so the security-relevant validator cannot drift
/// between `update` and `remove`.
///
/// # Errors
///
/// Returns a human-readable reason when the path must not be embedded.
#[cfg(windows)]
pub(crate) fn ensure_safe_windows_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(format!("refusing non-absolute path: {}", path.display()));
    }
    let text = path.to_string_lossy();
    if text
        .chars()
        .any(|c| matches!(c, '"' | '&' | '|' | '<' | '>' | '^' | '%' | '\r' | '\n'))
    {
        return Err(format!("refusing unsafe path: {}", path.display()));
    }
    Ok(())
}

#[cfg(unix)]
fn create_private_dir(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;

    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700);
    builder.create(path)
}

#[cfg(windows)]
fn create_private_dir(path: &Path) -> io::Result<()> {
    std::fs::create_dir(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The temp directory name is not the predictable `{prefix}-{pid}` form,
    /// so a local attacker cannot pre-create it.
    #[test]
    fn temp_dir_name_is_unpredictable() {
        let a = TempDir::create("cretopass-test").unwrap();
        let b = TempDir::create("cretopass-test").unwrap();
        assert_ne!(a.path(), b.path());
        assert!(a.path().exists());
    }

    /// Dropping a `TempDir` removes it.
    #[test]
    fn temp_dir_is_removed_on_drop() {
        let path = {
            let dir = TempDir::create("cretopass-test").unwrap();
            dir.path().to_path_buf()
        };
        assert!(!path.exists());
    }

    /// A kept `TempDir` survives the drop.
    #[test]
    fn kept_temp_dir_survives_drop() {
        let dir = TempDir::create("cretopass-test").unwrap();
        let path = dir.path().to_path_buf();
        dir.keep();
        assert!(path.exists());
        let _ = std::fs::remove_dir_all(&path);
    }

    /// `create_new_file` refuses to overwrite an existing file.
    #[test]
    fn create_new_file_refuses_existing() {
        let dir = TempDir::create("cretopass-test").unwrap();
        let path = dir.path().join("script.sh");
        create_new_file(&path).unwrap();
        let err = create_new_file(&path).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    /// An unset environment variable yields `None`.
    #[test]
    fn env_path_returns_none_for_unset() {
        assert_eq!(env_path("CRETOPASS_DEFINITELY_UNSET_VAR_9F3A"), None);
    }
}
