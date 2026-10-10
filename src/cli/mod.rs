//! CLI lifecycle commands for self-managed installations.
//!
//! This module implements `cretopass update` and `cretopass remove` — two
//! subcommands that operate on the installation itself rather than on secret
//! data. Both only work for GitHub Releases installations via `install.sh` /
//! `install.ps1`; pacman/AUR and `cargo install` are detected and refused.

pub mod detect;
pub mod remove;
pub mod update;
