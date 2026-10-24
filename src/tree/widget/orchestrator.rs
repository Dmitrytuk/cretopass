//! The one place that turns a modal's [`TreeAction`] into real work.
//!
//! Everything the Tree screen asks for lands here: disk I/O, the system
//! clipboard, screen transitions. The modals read and mutate the tree in memory
//! (see [`super::modal`]); this module performs what they cannot — anything
//! outside the process or the modal.

use std::io::{self, Write};

use super::TreeState;
use super::modal::TreeAction;

/// Performs `action`. The modal that produced it has already been taken out of
/// [`TreeState`], so dismissing it is the act of not putting it back.
pub fn execute(_state: &mut TreeState, action: TreeAction) -> io::Result<()> {
    match action {
        TreeAction::Close => Ok(()),
        TreeAction::CopyToClipboard { text } => {
            copy_to_clipboard(&text);
            Ok(())
        }
    }
}

/// Copies `text` to the system clipboard, trying the platform's helper in turn.
///
/// A missing helper (headless, or a platform without one) is silently ignored:
/// the copy is a best-effort convenience, not a failure the user must handle.
fn copy_to_clipboard(text: &str) {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "-b"],
        ]
    } else {
        return;
    };
    for args in cmds {
        if let Ok(mut child) = std::process::Command::new(args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            break;
        }
    }
}
