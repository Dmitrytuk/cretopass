//! Owns the terminal lifecycle.
//!
//! Every mode the app turns on — raw mode, the alternate screen, bracketed
//! paste and mouse capture — is turned off again through [`restore`], so
//! teardown is symmetric on normal exit, on error **and** on panic. Keeping the
//! modes in one module is also what lets the same [`restore`] be reused by the
//! panic hook installed by [`setup`].
//!
//! # Testing
//!
//! There is deliberately no test seam here. Every function mutates the real
//! terminal and the panic hook is process-global ([`Once`]), so a unit test
//! could only pretend to cover it (and would corrupt a running test session).
//! The ordering and partial-setup rollback paths are exercised manually: a
//! failed start must leave the shell usable, and every panic must restore it.
//! The pure pieces this module builds on (crossterm mode sequences, ratatui's
//! backend) are covered by their own tests.

use std::io::{self, Write};
use std::sync::Once;

use crossterm::cursor::Show;
use crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
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
/// Mouse capture is enabled here too, so the wheel and clicks reach the app
/// instead of being interpreted by the terminal. The trade-off is that the
/// terminal's native text selection is off while capture is on (most emulators
/// still select while `Shift` is held).
///
/// The panic hook is installed before any mode changes, so an unwinding panic
/// restores the terminal exactly like a normal exit.
///
/// # Errors
///
/// Returns an I/O error when raw mode, the alternate screen, bracketed paste or
/// mouse capture cannot be enabled. A partial setup is undone before returning,
/// so a failed start never leaves the terminal in raw mode.
pub(crate) fn setup() -> io::Result<Tui> {
    install_panic_hook();
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    if let Err(error) = execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture
    ) {
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

/// Installs the panic hook that restores the terminal before the default hook
/// prints the panic message.
///
/// Without it, a panic would unwind with raw mode, the alternate screen, mouse
/// capture and bracketed paste still on, leaving the shell unusable. Installing
/// at most once keeps a second [`setup`] call from chaining hooks.
fn install_panic_hook() {
    PANIC_HOOK_INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
    });
}

/// Guards [`install_panic_hook`], so the hook is set exactly once per process.
static PANIC_HOOK_INSTALLED: Once = Once::new();

/// Writes the mode-off sequence to `stdout`.
///
/// Raw mode goes first so the terminal is already line-buffered if writing the
/// escape sequences fails.
fn leave(stdout: &mut impl Write) {
    let _ = disable_raw_mode();
    let _ = execute!(
        stdout,
        DisableMouseCapture,
        DisableBracketedPaste,
        LeaveAlternateScreen,
        Show
    );
}
