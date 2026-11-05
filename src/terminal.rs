//! Owns the terminal lifecycle.
//!
//! Every mode the app turns on — raw mode, the alternate screen, bracketed
//! paste — is turned off again through [`restore`], so teardown is symmetric on
//! normal exit and on error. Keeping the modes in one module is also what lets
//! the same [`restore`] be reused for the panic hook.

use std::io::{self, Write};

use crossterm::cursor::Show;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

/// The terminal the app draws on.
pub(crate) type Tui = Terminal<CrosstermBackend<io::Stdout>>;

/// Puts the terminal into the app's mode and returns the handle to draw on.
///
/// # Errors
///
/// Returns an I/O error when raw mode, the alternate screen or bracketed paste
/// cannot be enabled. A partial setup is undone before returning, so a failed
/// start never leaves the terminal in raw mode.
pub(crate) fn setup() -> io::Result<Tui> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableBracketedPaste) {
        leave(&mut stdout);
        return Err(error);
    }
    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            // `stdout` was consumed into the failed backend; roll the modes back
            // through a fresh handle to the same terminal, exactly as `restore`.
            leave(&mut io::stdout());
            Err(error)
        }
    }
}

/// Undoes every mode [`setup`] enabled.
///
/// Safe to call even if [`setup`] failed or was never called: crossterm's
/// mode-off sequences are idempotent. Errors are ignored on purpose — the
/// terminal is going away, so there is nothing left to report them to.
pub(crate) fn restore() {
    leave(&mut io::stdout());
}

/// Writes the mode-off sequence to `stdout`.
///
/// Raw mode goes first so the terminal is already line-buffered if writing the
/// escape sequences fails.
fn leave(stdout: &mut impl Write) {
    let _ = disable_raw_mode();
    let _ = execute!(stdout, DisableBracketedPaste, LeaveAlternateScreen, Show);
}
