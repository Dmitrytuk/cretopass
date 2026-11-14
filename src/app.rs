//! Application wiring: the TUI entry point behind the CLI.
//!
//! The crate has a single binary target, so modules are declared at the crate
//! root in `main.rs` and this module owns no feature logic — it joins the
//! terminal lifecycle (`terminal`) to the Home screen (`vault`) and returns the
//! screen's result, so `cli::run` is the single place that reports the error and
//! chooses the exit code. Every feature lives in its own module below this
//! seam.

use std::io;

use crate::terminal;
use crate::vault;

/// Runs the home screen. Selecting "Open secret file" opens the tree
/// with the decrypted contents of the selected encrypted file.
pub fn run() -> io::Result<()> {
    let mut term = terminal::setup()?;
    let res = vault::run(&mut term);
    terminal::restore();
    res
}
