//! Writing the open tree to the file the user named, as plain text.

use crate::file_handling;
use crate::file_handling::ExportFormat;
use crate::theming::texts;

use super::super::{HomeState, Modal};
use super::errors;

/// Writes the open tree to `path` as `format`, reporting the result in an alert.
pub(super) fn export(state: &mut HomeState, path: &str, format: ExportFormat) {
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

    state.modal = Some(match file_handling::export_tree(&node, path, format) {
        Ok(out_path) => Modal::success(format!("{}{}", texts::MSG_EXPORT_SUCCESS, out_path)),
        Err(e) => Modal::error(errors::message(&e)),
    });
}
