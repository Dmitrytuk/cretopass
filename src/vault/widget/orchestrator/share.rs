//! Writing the open tree as a share bundle into the folder the user named.

use std::path::Path;

use crate::file_handling::share::share_tree;
use crate::theming::texts;

use super::super::{HomeState, Modal};
use super::errors;

/// Writes a bundle of the open tree into `dest`, encrypted with `passphrase`,
/// and reports the result in an alert.
///
/// The bundle takes the open file's content from memory — including edits the
/// autosave worker has not written yet — exactly like an export.
pub(super) fn share(state: &mut HomeState, dest: &str, passphrase: &str) {
    let Some(node) = state
        .active_vault
        .as_ref()
        .and_then(|active| active.node.clone())
    else {
        // Nothing to write: the destination was named, so say why it went
        // nowhere instead of dropping the user back on the menu silently.
        state.modal = Some(Modal::error(texts::MSG_NO_OPEN_FILE.to_string()));
        return;
    };
    let base_path = state
        .active_vault
        .as_ref()
        .and_then(|active| active.path.clone());

    state.modal = Some(
        match share_tree(
            &node,
            Path::new(dest),
            base_path.as_deref().map(Path::new),
            None,
            passphrase,
        ) {
            Ok(bundle) => {
                Modal::success(format!("{}{}", texts::MSG_SHARE_SUCCESS, bundle.display()))
            }
            Err(e) => Modal::error(errors::message(&e)),
        },
    );
}
