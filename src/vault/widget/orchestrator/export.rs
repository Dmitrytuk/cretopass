//! Writing the open tree beside its secret file as plain text.

use crate::file_handling;
use crate::file_handling::export::ExportFormat;
use crate::theming::texts;

use super::super::{HomeState, Modal};
use super::errors;
/// Writes the open tree to a plain-text file beside the secret file, reporting
/// the result in an alert.
pub(super) fn export(state: &mut HomeState, format: ExportFormat) {
    let Some((node, path)) = state
        .active_vault
        .as_ref()
        .and_then(|active| Some((active.node.clone()?, active.path.clone()?)))
    else {
        // Nothing is open: the wizard was taken out, so there is nothing to show.
        return;
    };

    state.modal = Some(
        match file_handling::export::export_tree(&node, &path, format) {
            Ok(out_path) => Modal::alert(format!("{}{}", texts::MSG_EXPORT_SUCCESS, out_path)),
            Err(e) => Modal::alert(errors::message(&e)),
        },
    );
}
