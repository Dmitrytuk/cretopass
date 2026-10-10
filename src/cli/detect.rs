use std::path::{Path, PathBuf};

/// Identifies how cretopass was installed, based on the running binary's path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallSource {
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
    /// Detect the install source from the running executable's path.
    pub fn detect() -> Self {
        let exe = match std::env::current_exe().and_then(|p| p.canonicalize()) {
            Ok(p) => p,
            Err(_) => return Self::Unknown,
        };

        if is_self_managed(&exe) {
            return Self::SelfManaged;
        }
        if is_pacman(&exe) {
            return Self::Pacman;
        }
        if is_cargo_install(&exe) {
            return Self::CargoInstall;
        }
        Self::Unknown
    }

    /// Require a self-managed install.
    ///
    /// Returns an error message with a hint for the appropriate package
    /// manager if the install source is foreign.
    pub fn require_self_managed(self) -> Result<(), String> {
        match self {
            Self::SelfManaged => Ok(()),
            Self::Pacman => Err("cretopass was installed via pacman.\n\
                 Use `sudo pacman -Syu cretopass` to update.\n\
                 Use `sudo pacman -R cretopass` to remove."
                .to_string()),
            Self::CargoInstall => Err("cretopass was installed via cargo.\n\
                 Use `cargo install --force cretopass` to update.\n\
                 Use `cargo uninstall cretopass` to remove."
                .to_string()),
            Self::Unknown => Err("cannot determine how cretopass was installed.\n\
                 Update and remove are only supported for installations from GitHub Releases\n\
                 (via install.sh or install.ps1)."
                .to_string()),
        }
    }
}

/// Returns the home directory, or an empty path if not available.
fn home_dir() -> PathBuf {
    #[cfg(unix)]
    {
        std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default()
    }
}

#[cfg(unix)]
fn is_self_managed(exe: &Path) -> bool {
    let home = home_dir();
    if home.as_os_str().is_empty() {
        return false;
    }
    exe.starts_with(home.join(".local/bin"))
}

#[cfg(windows)]
fn is_self_managed(exe: &Path) -> bool {
    if let Ok(d) = std::env::var("LOCALAPPDATA") {
        return exe.starts_with(Path::new(&d).join("Programs").join("cretopass"));
    }
    false
}

fn is_pacman(exe: &Path) -> bool {
    exe.starts_with("/usr/bin/")
}

fn is_cargo_install(exe: &Path) -> bool {
    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home_dir().join(".cargo"));
    if cargo_home.as_os_str().is_empty() {
        return false;
    }
    exe.starts_with(cargo_home.join("bin"))
}
