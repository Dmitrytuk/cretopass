//! CLI lifecycle commands for self-managed installations.
//!
//! This module implements the two subcommands that operate on the installation
//! itself rather than on secret data: `cretopass update` and `cretopass remove`.
//! Both only work for GitHub Releases installations via `install.sh` /
//! `install.ps1`; pacman/AUR and `cargo install` are detected and refused.
//!
//! The sub-modules are private; [`LifecycleCommand`] is the only entry point so
//! `main.rs` stays a pure parser + dispatcher (see ADR 0009).

mod detect;
mod layout;
mod remove;
mod update;

use std::process::ExitCode;

/// Exit code used when `update`/`remove` refuse a foreign installation.
pub const EXIT_FOREIGN_INSTALL: u8 = 2;

/// A lifecycle subcommand that mutates the installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCommand {
    /// Download and run the platform installer (`cretopass update`).
    Update,
    /// Remove every installed artifact (`cretopass remove`).
    Remove,
}

impl LifecycleCommand {
    /// Parses a raw CLI argument into a lifecycle subcommand.
    ///
    /// Returns `None` for anything that is not `update` or `remove`.
    #[must_use]
    pub fn parse(arg: &str) -> Option<Self> {
        match arg {
            "update" => Some(Self::Update),
            "remove" => Some(Self::Remove),
            _ => None,
        }
    }

    /// Runs the subcommand and returns its [`ExitCode`].
    #[must_use]
    pub fn run(self) -> ExitCode {
        match self {
            Self::Update => update::run_update(),
            Self::Remove => remove::run_remove(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known subcommands parse to their variant.
    #[test]
    fn parses_known_subcommands() {
        assert_eq!(
            LifecycleCommand::parse("update"),
            Some(LifecycleCommand::Update)
        );
        assert_eq!(
            LifecycleCommand::parse("remove"),
            Some(LifecycleCommand::Remove)
        );
    }

    /// Unknown arguments are rejected.
    #[test]
    fn rejects_unknown_subcommands() {
        assert_eq!(LifecycleCommand::parse("install"), None);
        assert_eq!(LifecycleCommand::parse(""), None);
    }

    /// The foreign-install exit code is the documented value.
    #[test]
    fn foreign_install_exit_code_is_two() {
        assert_eq!(EXIT_FOREIGN_INSTALL, 2);
    }
}
