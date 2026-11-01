use std::fmt;
use std::path::Path;

use super::layout;

/// Identifies how cretopass was installed, based on the running binary's path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InstallSource {
    /// Installed via install.sh or install.ps1 (self-managed GitHub Releases).
    SelfManaged,
    /// Installed via system package manager (pacman/AUR).
    Pacman,
    /// Installed via `cargo install`.
    CargoInstall,
    /// Unknown installation method — refuse update/remove.
    Unknown,
}

impl InstallSource {
    /// Detects the install source from the running executable's path.
    pub(crate) fn detect() -> Self {
        match std::env::current_exe().and_then(|p| p.canonicalize()) {
            Ok(exe) => Self::detect_from(&exe),
            Err(_) => Self::Unknown,
        }
    }

    /// Detects the install source from an explicit executable path.
    ///
    /// Split out from [`Self::detect`] so the path predicates can be tested
    /// without depending on where the running binary happens to live.
    ///
    /// The order is significant: a self-managed install that also happens to
    /// sit under `/usr/bin` or a cargo prefix must win.
    pub(crate) fn detect_from(exe: &Path) -> Self {
        if is_self_managed(exe) {
            Self::SelfManaged
        } else if is_pacman(exe) {
            Self::Pacman
        } else if is_cargo_install(exe) {
            Self::CargoInstall
        } else {
            Self::Unknown
        }
    }

    /// Requires a self-managed install, or returns a [`ForeignInstall`] error.
    ///
    /// # Errors
    ///
    /// Returns [`ForeignInstall`] for every source other than
    /// [`InstallSource::SelfManaged`].
    pub(crate) fn require_self_managed(self) -> Result<(), ForeignInstall> {
        match self {
            Self::SelfManaged => Ok(()),
            other => Err(ForeignInstall(other)),
        }
    }
}

/// A refusal to run `update`/`remove` against a non-self-managed install.
///
/// Carries the detected [`InstallSource`] so callers can distinguish a
/// `pacman` install from a `cargo install` without parsing user-facing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ForeignInstall(pub InstallSource);

impl fmt::Display for ForeignInstall {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            InstallSource::SelfManaged => write!(f, "cretopass is self-managed"),
            InstallSource::Pacman => write!(
                f,
                "cretopass was installed via pacman.\n\
                 Use `sudo pacman -Syu cretopass` to update.\n\
                 Use `sudo pacman -R cretopass` to remove."
            ),
            InstallSource::CargoInstall => write!(
                f,
                "cretopass was installed via cargo.\n\
                 Use `cargo install --force cretopass` to update.\n\
                 Use `cargo uninstall cretopass` to remove."
            ),
            InstallSource::Unknown => write!(
                f,
                "cannot determine how cretopass was installed.\n\
                 Update and remove are only supported for installations from GitHub Releases\n\
                 (via install.sh or install.ps1)."
            ),
        }
    }
}

impl std::error::Error for ForeignInstall {}

/// Returns true when the binary lives in the self-managed install directory.
///
/// The prefix is canonicalized because `detect` compares against a
/// canonicalized executable path; without this, an install under a symlinked
/// home or `/var` (macOS) would be misreported as unknown.
fn is_self_managed(exe: &Path) -> bool {
    match layout::home_dir() {
        Some(home) => {
            let prefix = layout::bin_dir(&home);
            let prefix = prefix.canonicalize().unwrap_or(prefix);
            exe.starts_with(prefix)
        }
        None => false,
    }
}

/// Returns true when the binary lives in `/usr/bin` (pacman ownership).
#[cfg(unix)]
fn is_pacman(exe: &Path) -> bool {
    exe.starts_with("/usr/bin")
}

/// Package-manager detection is Unix-specific; it can never match on Windows.
#[cfg(not(unix))]
fn is_pacman(_exe: &Path) -> bool {
    false
}

/// Returns true when the binary lives in a `cargo install` prefix.
fn is_cargo_install(exe: &Path) -> bool {
    let cargo_home = layout::env_path("CARGO_HOME")
        .or_else(|| layout::home_dir().map(|home| home.join(".cargo")));
    match cargo_home {
        Some(dir) => exe.starts_with(dir.join("bin")),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A binary under `$HOME/.local/bin` is self-managed.
    #[test]
    #[cfg(unix)]
    fn detects_self_managed_under_home_local_bin() {
        let home = layout::home_dir().expect("HOME is set in the test environment");
        let exe = layout::bin_dir(&home).join(layout::BIN_NAME);
        assert_eq!(
            InstallSource::detect_from(&PathBuf::from(&exe)),
            InstallSource::SelfManaged
        );
    }

    /// The self-managed prefix wins over the `/usr/bin` pacman prefix.
    #[test]
    #[cfg(unix)]
    fn self_managed_wins_over_pacman() {
        // A path that starts with `$HOME/.local/bin` cannot also start with
        // `/usr/bin`, so this asserts the ordering intent directly.
        let home = layout::home_dir().expect("HOME is set in the test environment");
        assert!(!layout::bin_dir(&home).starts_with("/usr/bin"));
    }

    /// `/usr/bin/cretopass` is detected as a pacman install on Unix.
    #[test]
    #[cfg(unix)]
    fn detects_pacman_under_usr_bin() {
        assert_eq!(
            InstallSource::detect_from(Path::new("/usr/bin/cretopass")),
            InstallSource::Pacman
        );
    }

    /// A binary under a cargo prefix is detected as a cargo install.
    #[test]
    fn detects_cargo_install() {
        let cargo_home = layout::env_path("CARGO_HOME")
            .or_else(|| layout::home_dir().map(|home| home.join(".cargo")));
        match cargo_home {
            Some(dir) => assert_eq!(
                InstallSource::detect_from(&dir.join("bin/cretopass")),
                InstallSource::CargoInstall
            ),
            None => eprintln!("skipping: no cargo home could be determined"),
        }
    }

    /// An unrelated path is unknown.
    #[test]
    fn unknown_path_is_unknown() {
        assert_eq!(
            InstallSource::detect_from(Path::new("/nowhere/cretopass")),
            InstallSource::Unknown
        );
    }

    /// `require_self_managed` accepts a self-managed install.
    #[test]
    fn require_self_managed_accepts_self_managed() {
        assert!(InstallSource::SelfManaged.require_self_managed().is_ok());
    }

    /// A foreign install yields a typed error carrying the source.
    #[test]
    fn require_self_managed_reports_foreign_source() {
        let err = InstallSource::CargoInstall
            .require_self_managed()
            .unwrap_err();
        assert_eq!(err.0, InstallSource::CargoInstall);
        assert!(err.to_string().contains("cargo"));
    }
}
