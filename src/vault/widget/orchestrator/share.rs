//! Writing the open tree as a share bundle into the folder the user named.

use std::io;
use std::path::Path;

use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::file_handling::share::share_tree;
use crate::theming::Status;
use crate::theming::texts;

use super::super::{HomeState, Modal, run_with_spinner};
use super::errors;

/// Writes a bundle of the open tree into `dest`, encrypted with `passphrase`,
/// and reports the result in an alert.
///
/// The bundle takes the open file's content from memory — including edits the
/// autosave worker has not written yet — exactly like an export. The write runs
/// off the UI thread so the title bar keeps animating its `Saving` status while
/// the bundle is re-encrypted and fsynced (see
/// [ADR 0011](../../../../docs/adr/0011-animated-status-spinner.md)).
///
/// # Errors
///
/// Returns an I/O error when the UI cannot be redrawn while the worker runs;
/// the write itself is reported in the alert it installs.
pub(super) fn share<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    dest: &str,
    passphrase: &str,
) -> io::Result<()> {
    let Some(node) = state
        .active_vault
        .as_ref()
        .and_then(|active| active.node.clone())
    else {
        // Nothing to write: the destination was named, so say why it went
        // nowhere instead of dropping the user back on the menu silently.
        state.modal = Some(Modal::error(texts::MSG_NO_OPEN_FILE.to_string()));
        return Ok(());
    };
    let base_path = state
        .active_vault
        .as_ref()
        .and_then(|active| active.path.clone());
    let dest = dest.to_string();
    let passphrase = passphrase.to_string();

    let outcome = run_with_spinner(terminal, state, Status::Saving, move || {
        share_tree(
            &node,
            Path::new(&dest),
            base_path.as_deref().map(Path::new),
            None,
            &passphrase,
        )
    })?;

    state.modal = Some(match outcome {
        Ok(bundle) => Modal::success(format!("{}{}", texts::MSG_SHARE_SUCCESS, bundle.display())),
        Err(e) => Modal::error(errors::message(&e)),
    });
    Ok(())
}
