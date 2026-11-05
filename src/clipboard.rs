//! Gateway to the system clipboard.
//!
//! [`copy_text`] is the single place secrets reach the OS clipboard. It uses
//! [`arboard`] on every platform (Windows included, via Win32 `CF_UNICODETEXT`,
//! so non-ASCII text survives), and falls back to the platform's command-line
//! helper only when arboard cannot provide one.
//!
//! Unlike the rest of the app's I/O it does not go through `file_handling`: the
//! clipboard is not a document. The gateway is nonetheless the one module that
//! owns the failure taxonomy ([`ClipboardError`]) and the process-wide handle.

use std::fmt;
use std::io::Write;
use std::sync::{Mutex, OnceLock};

/// The one cached clipboard handle for the process.
///
/// On X11 and Wayland the clipboard contents are served by the owning process,
/// so the handle must outlive the write: dropping it right after `set_text`
/// makes the value vanish. Caching it here keeps the value pasteable while the
/// app runs. On Windows and macOS the handle is cheap and the cache is harmless.
static CELL: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();

/// Copies `text` to the system clipboard.
///
/// Tries arboard first and the platform helper ladder second. Success keeps the
/// arboard handle cached; an initialisation failure leaves it uncached (the next
/// call retries) and a write failure drops it (the next call re-initialises).
///
/// # Errors
/// Returns [`ClipboardError`] when neither arboard nor a helper could take the
/// value. A helper is only tried when arboard fails, and only on Linux/macOS.
pub(crate) fn copy_text(text: &str) -> Result<(), ClipboardError> {
    match copy_with_arboard(text) {
        Ok(()) => Ok(()),
        // arboard failed: fall back to the platform helper. If no helper takes
        // it, report arboard's error, which describes the real problem.
        Err(error) => {
            if copy_with_helper(text) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

/// Copies `text` through the cached [`arboard::Clipboard`].
///
/// Opens the handle on first use and keeps it on success. A poisoned mutex is
/// recovered from: the cached handle carries no invariant that a panic could
/// have broken, so refusing to use it would only break copying for good.
fn copy_with_arboard(text: &str) -> Result<(), ClipboardError> {
    let cell = CELL.get_or_init(|| Mutex::new(None));
    let mut guard = match cell.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if guard.is_none() {
        match arboard::Clipboard::new() {
            Ok(clipboard) => *guard = Some(clipboard),
            Err(error) => return Err(ClipboardError::Init(error.to_string())),
        }
    }

    let result = guard
        .as_mut()
        .expect("a handle was just installed")
        .set_text(text);
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            // The handle may be wedged (the display went away, the channel
            // broke): drop it so the next copy opens a fresh one.
            *guard = None;
            Err(ClipboardError::Write(error.to_string()))
        }
    }
}

/// Copies `text` through the platform's helper command, if one exists.
///
/// Returns `false` when no helper is configured for the platform or every
/// helper failed. A helper only counts as success when it exits zero: a
/// present-but-broken helper (a headless `xclip` that cannot open the display)
/// must not masquerade as a copy, so the ladder falls through to the next one.
fn copy_with_helper(text: &str) -> bool {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "-b"],
        ]
    } else {
        return false;
    };

    for args in cmds {
        if let Ok(mut child) = std::process::Command::new(args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            // Take the pipe out of the child and let it drop at the end of the
            // block: the helper (xclip/xsel) reads stdin to EOF before it takes
            // the selection, so the write end must be closed before `wait`.
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            if child.wait().is_ok_and(|status| status.success()) {
                return true;
            }
        }
    }
    false
}

/// Why copying to the system clipboard failed.
///
/// The only project-owned copy, so a caller can render it without naming
/// `arboard`. The reason strings are arboard's already-rendered messages; they
/// describe the environment (no display, no server), not the secret.
#[derive(Debug)]
pub(crate) enum ClipboardError {
    /// The clipboard could not be opened at all (no display or server).
    Init(String),
    /// The clipboard was opened but the value could not be written.
    Write(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(reason) => write!(f, "cannot open the system clipboard: {reason}"),
            Self::Write(reason) => write!(f, "cannot write to the system clipboard: {reason}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_init_error_names_the_failed_step() {
        assert_eq!(
            ClipboardError::Init("no display".to_string()).to_string(),
            "cannot open the system clipboard: no display"
        );
    }

    #[test]
    fn a_write_error_names_the_failed_step() {
        assert_eq!(
            ClipboardError::Write("broken pipe".to_string()).to_string(),
            "cannot write to the system clipboard: broken pipe"
        );
    }
}
