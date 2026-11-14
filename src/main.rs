//! Cretopass: a terminal-based secret keeper.
//!
//! The crate has a single binary target — there is no library and therefore no
//! published API. [`main`] only hands control to [`cli::run`], which parses the
//! command line and either runs a lifecycle subcommand or starts the TUI
//! through [`app::run`]. Every other module is application code.

#![deny(unsafe_code)]

mod app;
mod cli;
mod clipboard;
mod config;
mod crypt;
mod cursor;
mod file_browser;
mod file_handling;
mod generator;
mod hitbox;
mod share;
mod terminal;
mod theming;
mod tree;
mod vault;

use std::process::ExitCode;

fn main() -> ExitCode {
    cli::run()
}
