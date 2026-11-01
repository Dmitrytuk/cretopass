//! Acting on the secret file: unlocking it, creating it, re-keying it, and
//! handing it to the Tree.
//!
//! Everything here works on the vault that is open (or about to be) and is reached
//! from [`super::execute`].

use std::io;

use ratatui::Terminal;
use ratatui::backend::Backend;
use zeroize::Zeroizing;

use crate::file_handling;
use crate::theming::Status;
use crate::theming::texts;
use crate::tree::app_tree::SaveTarget;
use crate::tree::node::Node;
use crate::vault::active_file::ActiveVaultState;
use crate::vault::utils;

use super::super::modal::{CreateSecretPrompt, UnlockPrompt};
use super::super::{HomeState, Modal, run_with_spinner};
use super::errors;
/// Re-encrypts the open secret file with the typed replacement master password.
///
/// Validation lives here, not in the prompt: the "current password" field is
/// checked against the open vault, which only this module holds. The order of the
/// three checks is the one users are used to — current password, then emptiness,
/// then a mismatched pair.
pub(super) fn change_master_password<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    modal: Option<Modal>,
) -> io::Result<()> {
    let Some(Modal::ChangePassword(mut prompt)) = modal else {
        return Ok(());
    };

    let current_matches = prompt.current().as_bytes()
        == state
            .active_vault
            .as_ref()
            .map(|active| active.passphrase.as_slice())
            .unwrap_or(&[]);

    let error = if !current_matches {
        Some(texts::MSG_CURRENT_PW_INCORRECT.to_string())
    } else if prompt.new_password().is_empty() || prompt.confirmation().is_empty() {
        Some(texts::MSG_PW_EMPTY.to_string())
    } else if prompt.new_password() != prompt.confirmation() {
        Some(texts::MSG_PW_MISMATCH.to_string())
    } else {
        None
    };

    if let Some(message) = error {
        prompt.reject(message);
        state.modal = Some(Modal::ChangePassword(prompt));
        return Ok(());
    }

    let Some((path, node)) = state
        .active_vault
        .as_ref()
        .and_then(|active| Some((active.path.clone()?, active.node.clone()?)))
    else {
        return Ok(());
    };

    let new_password = Zeroizing::new(prompt.new_password().as_bytes().to_vec());
    let save_node = node.clone();
    let save_path = path.clone();
    let (outcome, new_password) = run_with_spinner(terminal, state, Status::Saving, move || {
        let outcome = file_handling::save_tree(&save_node, &save_path, &new_password);
        (outcome, new_password)
    })?;

    match outcome {
        Err(e) => {
            prompt.reject_and_clear(errors::message(&e));
            state.modal = Some(Modal::ChangePassword(prompt));
            Ok(())
        }
        Ok(()) => {
            state.active_vault.as_mut().unwrap().passphrase = new_password.clone();
            open_tree(state, terminal, node, Some(path), new_password)
        }
    }
}

/// The user picked where a new secret file should go: ask for its master password,
/// or report why it cannot be created there.
pub(super) fn choose_file_to_create(state: &mut HomeState, file_path: String) {
    if std::path::Path::new(&file_path).exists() {
        state.modal = Some(Modal::error(texts::MSG_FILE_EXISTS.to_string()));
    } else if file_handling::probe_path_writable(&file_path).is_ok() {
        state.modal = Some(Modal::CreateSecret(CreateSecretPrompt::new(file_path)));
    } else {
        state.modal = Some(Modal::error(texts::MSG_CANNOT_WRITE.to_string()));
    }
}

/// The user picked a file to open: ask for its master password, or report why it
/// is not a secret file.
pub(super) fn choose_file_to_open(state: &mut HomeState, file_path: String) {
    state.modal = Some(match file_handling::validate_encrypted_file(&file_path) {
        Ok(data) => Modal::Unlock(UnlockPrompt::new(file_path, data)),
        Err(e) => Modal::error(errors::message(&e)),
    });
}

/// Creates the file the create dialog was opened for and opens it in the Tree.
///
/// A failed write re-shows the dialog with the error, keeping the typed pair.
pub(super) fn create_secret<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    modal: Option<Modal>,
) -> io::Result<()> {
    let Some(Modal::CreateSecret(mut prompt)) = modal else {
        return Ok(());
    };

    let file_path = prompt.file_path().to_string();
    let passphrase = prompt.typed();
    let (outcome, file_path, passphrase) =
        run_with_spinner(terminal, state, Status::Saving, move || {
            let outcome = file_handling::create_secret_file(&file_path, &passphrase);
            (outcome, file_path, passphrase)
        })?;

    match outcome {
        Err(e) => {
            prompt.reject(errors::message(&e));
            state.modal = Some(Modal::CreateSecret(prompt));
            Ok(())
        }
        Ok(node) => {
            let passphrase = Zeroizing::new(passphrase.as_bytes().to_vec());
            state.active_vault = Some(ActiveVaultState {
                path: Some(file_path.clone()),
                passphrase: passphrase.clone(),
                node: Some(node.clone()),
            });
            let _ = utils::push_recent_secret_file(&mut state.config, &file_path);
            open_tree(state, terminal, node, Some(file_path), passphrase)
        }
    }
}

/// Creates `file_path` with an empty master password and opens it in the Tree.
pub(super) fn create_without_password<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    file_path: String,
) -> io::Result<()> {
    let (outcome, file_path) = run_with_spinner(terminal, state, Status::Saving, move || {
        let outcome = file_handling::create_secret_file(&file_path, "");
        (outcome, file_path)
    })?;

    match outcome {
        Err(e) => {
            state.modal = Some(Modal::error(errors::message(&e)));
            Ok(())
        }
        Ok(node) => {
            let passphrase = Zeroizing::new(Vec::new());
            state.active_vault = Some(ActiveVaultState {
                path: Some(file_path.clone()),
                passphrase: passphrase.clone(),
                node: Some(node.clone()),
            });
            let _ = utils::push_recent_secret_file(&mut state.config, &file_path);
            open_tree(state, terminal, node, Some(file_path), passphrase)
        }
    }
}

/// Reopens the already-decrypted vault in the Tree, if there is one.
pub(super) fn launch_tree<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let Some((node, path, passphrase)) = state.active_vault.as_ref().and_then(|active| {
        Some((
            active.node.clone()?,
            active.path.clone(),
            active.passphrase.clone(),
        ))
    }) else {
        return Ok(());
    };

    open_tree(state, terminal, node, path, passphrase)
}

/// Opens `node` in the Tree screen, autosaving it back to `path` when it has one.
///
/// Every Home flow that ends up in the Tree goes through here, so the tree is
/// always seeded from the same place.
pub fn open_tree<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    node: Node,
    path: Option<String>,
    passphrase: Zeroizing<Vec<u8>>,
) -> io::Result<()> {
    let target = path.map(|path| SaveTarget { path, passphrase });
    let mut tree_state = crate::tree::widget::TreeState::new(node);
    tree_state.display_full_path = state.config.display_full_path;
    tree_state.generator = state.generator;
    tree_state.browser_session = state.browser_session.clone();
    run_tree(state, terminal, tree_state, target)
}

/// Runs the Tree screen and merges what it leaves behind back into Home.
///
/// On a normal exit the autosave worker may have written a newer root to disk than
/// the one Home originally decrypted; keeping the in-memory copy in sync stops
/// later home operations (change password, import) from overwriting those edits
/// with stale data. A force-quit ([`ExitStatus::Unsaved`](crate::tree::app_tree::ExitStatus::Unsaved)) instead drops the active
/// vault, so no later home action can persist the discarded tree.
pub(super) fn run_tree<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    mut tree_state: crate::tree::widget::TreeState,
    target: Option<SaveTarget>,
) -> io::Result<()> {
    use crate::tree::app_tree::ExitStatus;

    if let Some(target) = &target {
        tree_state.active_vault_path = Some(target.path.clone());
    }
    let result = crate::tree::app_tree::run_app(terminal, &mut tree_state, target);

    // Generator settings are per app run: copy them back even when the tree
    // exits with an error or the user discards changes.
    state.generator = tree_state.generator;
    // Same for the File Browser's memory: an export named from inside the tree
    // leaves Home starting where the user last browsed.
    state.browser_session = tree_state.browser_session;
    let return_to_menu = tree_state.return_to_menu;
    match result {
        Ok(ExitStatus::Saved) => {
            if let Some(active) = state.active_vault.as_mut() {
                active.node = Some(tree_state.root);
            }
            if !return_to_menu {
                state.quit = true;
            }
            Ok(())
        }
        Ok(ExitStatus::Unsaved) => {
            state.active_vault = None;
            if !return_to_menu {
                state.quit = true;
            }
            Ok(())
        }
        // Sync even on error: a later retry from Home must not start from a
        // stale copy that could overwrite edits already written by the worker.
        Err(error) => {
            if let Some(active) = state.active_vault.as_mut() {
                active.node = Some(tree_state.root);
            }
            Err(error)
        }
    }
}

/// Decrypts the file the unlock prompt was opened for and opens it in the Tree.
///
/// A wrong master password re-shows the prompt with the error, keeping what was
/// typed so the user can correct it.
pub(super) fn unlock<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    modal: Option<Modal>,
) -> io::Result<()> {
    let Some(Modal::Unlock(mut prompt)) = modal else {
        return Ok(());
    };

    let passphrase = prompt.typed();
    let file_data = prompt.file_data().to_vec();
    let (outcome, passphrase) = run_with_spinner(terminal, state, Status::Loading, move || {
        let outcome = file_handling::decrypt_to_node(&file_data, &passphrase);
        (outcome, passphrase)
    })?;

    match outcome {
        Err(e) => {
            prompt.reject(errors::message(&e));
            state.modal = Some(Modal::Unlock(prompt));
            Ok(())
        }
        Ok(node) => {
            let file_path = prompt.file_path().to_string();
            let passphrase = Zeroizing::new(passphrase.as_bytes().to_vec());
            state.active_vault = Some(ActiveVaultState {
                path: Some(file_path.clone()),
                passphrase: passphrase.clone(),
                node: Some(node.clone()),
            });
            let _ = utils::push_recent_secret_file(&mut state.config, &file_path);
            open_tree(state, terminal, node, Some(file_path), passphrase)
        }
    }
}
