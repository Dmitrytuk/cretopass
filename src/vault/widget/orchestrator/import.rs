//! Importing a plain-text tree into the open vault.
//!
//! The file is read and parsed off the UI thread, then filed under a
//! timestamped wrapper inside the open Root, so one import never collides with
//! the next.

use std::io;

use ratatui::Terminal;
use ratatui::backend::Backend;
use zeroize::Zeroizing;

use crate::file_handling;
use crate::theming::Status;
use crate::theming::texts;
use crate::tree::node::Node;

use super::super::{HomeState, Modal, run_with_spinner};
use super::errors;
/// Merges the tree stored in `import_path` into the open vault and saves it.
///
/// The imported tree is filed under a timestamped wrapper node inside the open
/// Root, so one import never collides with the next.
pub(super) fn import<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    import_path: String,
) -> io::Result<()> {
    // Read and parse on the worker: an import may be as large as the file cap
    // and its parse is not cheap, so the event thread must not do it. The
    // spinner drains input, so a stray key cannot start a second import.
    let path_to_read = import_path.clone();
    let imported = run_with_spinner(terminal, state, Status::Loading, move || {
        file_handling::import_tree(&path_to_read)
    })?;

    let imported = match imported {
        Ok(node) => node,
        Err(e) => {
            state.modal = Some(Modal::error(errors::message(&e)));
            return Ok(());
        }
    };

    let Some((save_path, save_passphrase, save_node)) = wrap_import(state, &import_path, imported)
    else {
        return Ok(());
    };

    let outcome = run_with_spinner(terminal, state, Status::Saving, move || {
        file_handling::save_tree(&save_node, &save_path, &save_passphrase)
    })?;

    state.modal = Some(match outcome {
        Ok(()) => Modal::success(texts::MSG_IMPORT_SUCCESS.to_string()),
        Err(e) => Modal::error(errors::message(&e)),
    });
    Ok(())
}

/// Files `imported` under a timestamped wrapper inside the open Root, returning
/// what to save.
///
/// Returns `None` after showing an alert when there is nothing to import into,
/// or nothing to save to.
pub(super) fn wrap_import(
    state: &mut HomeState,
    import_path: &str,
    imported: Node,
) -> Option<(String, Zeroizing<Vec<u8>>, Node)> {
    let mut problem = None;
    let target = match state.active_vault.as_mut() {
        None => {
            problem = Some(texts::MSG_IMPORT_NO_OPEN_FILE);
            None
        }
        Some(active) => match active.node.as_mut() {
            // Defensive: an open vault always carries its tree.
            None => None,
            Some(root) => {
                let mut wrapper = Node::new(wrapper_name(import_path));
                wrapper.children = vec![imported];
                root.add_child(wrapper);

                match active.path.clone() {
                    Some(path) => Some((path, active.passphrase.clone(), root.clone())),
                    None => {
                        problem = Some(texts::MSG_NO_FILE_PATH);
                        None
                    }
                }
            }
        },
    };

    if let Some(message) = problem {
        state.modal = Some(Modal::error(message.to_string()));
    }
    target
}

/// The wrapper node an import is filed under: the imported file's stem and the
/// local time, so repeated imports of the same file stay apart.
pub(super) fn wrapper_name(import_path: &str) -> String {
    let stem = std::path::Path::new(import_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| texts::IMPORT_NAME_FALLBACK.to_string());
    let stamp = chrono::Local::now().format(texts::IMPORT_NAME_STAMP_FORMAT);
    format!("{}{stem}_{stamp}", texts::IMPORT_NAME_PREFIX)
}
