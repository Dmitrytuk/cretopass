//! The one place that turns a modal's [`HomeAction`] into real work.
//!
//! Everything a Home modal asks for lands here: disk I/O, the blocking spinner,
//! configuration writes, screen transitions and launching the Tree. The modals
//! stay pure input (see [`crate::vault::widget::modal`]); this module is allowed
//! to be shallow, because all it does is perform intentions.

use std::io;

use ratatui::Terminal;
use ratatui::backend::Backend;
use zeroize::Zeroizing;

use crate::file_handling;
use crate::file_handling::export::ExportFormat;
use crate::theming::Status;
use crate::theming::texts;
use crate::theming::{Palette, set_palette, toggled_scheme};
use crate::tree::app_tree::SaveTarget;
use crate::tree::node::Node;
use crate::vault::active_file::ActiveVaultState;
use crate::vault::utils;

use super::modal::{ChangePasswordPrompt, Confirmation, CreateSecretPrompt, HomeAction, Setting};
use super::{HomeModal, HomeState, run_tree, run_with_spinner};

/// Performs `action`. The modal that produced it has already been removed from
/// [`HomeState`]: "close" means leaving it that way, a transition installs the
/// modal it names, and `modal` is handed back for the actions that must leave the
/// dialog open on screen.
pub fn execute<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    action: HomeAction,
    modal: HomeModal,
) -> io::Result<()> {
    match action {
        HomeAction::Close => Ok(()),
        HomeAction::DeleteRecent { path } => {
            delete_recent(state, &path);
            Ok(())
        }
        HomeAction::ChangePasswordPrompt => {
            state.modal = Some(HomeModal::ChangePassword(ChangePasswordPrompt::new()));
            Ok(())
        }
        HomeAction::ReopenCreateSecret { file_path } => {
            state.modal = Some(HomeModal::CreateSecret(CreateSecretPrompt::new(file_path)));
            Ok(())
        }
        HomeAction::ConfirmNoPassphrase { file_path } => {
            state.modal = Some(HomeModal::Confirmation(Confirmation::no_passphrase(
                file_path,
            )));
            Ok(())
        }
        HomeAction::CreateWithoutPassword { file_path } => {
            create_without_password(state, terminal, file_path)
        }
        HomeAction::Unlock => unlock(state, terminal, modal),
        HomeAction::CreateSecret => create_secret(state, terminal, modal),
        HomeAction::ChangeMasterPassword => change_master_password(state, terminal, modal),
        HomeAction::Export { format } => {
            export(state, format);
            Ok(())
        }
        HomeAction::ToggleSetting(setting) => {
            apply_setting(state, setting);
            // The dialog stays open so the user sees the setting take effect.
            state.modal = Some(modal);
            Ok(())
        }
        HomeAction::SaveConfig => {
            let _ = utils::save_config(&state.config);
            state.modal = Some(HomeModal::alert(texts::MSG_CONFIG_SAVED.to_string()));
            Ok(())
        }
        HomeAction::DiscardConfig => {
            state.config = utils::load_config();
            // The scheme is applied live on toggle, so discarding the config must
            // restore the colours too (ADR 0019: "Enter persists and Esc reverts").
            set_palette(Palette::from_name(&state.config.palette_name));
            Ok(())
        }
    }
}

/// Flips one setting. The colour scheme is applied at once, so the user sees it
/// while the dialog is still open; it is persisted only on save.
fn apply_setting(state: &mut HomeState, setting: Setting) {
    match setting {
        Setting::DisplayFullPath => {
            state.config.display_full_path = !state.config.display_full_path
        }
        Setting::OpenLastFileOnStartup => {
            state.config.open_last_opened_file_on_startup =
                !state.config.open_last_opened_file_on_startup;
        }
        Setting::ColourScheme => {
            state.config.palette_name = toggled_scheme(&state.config.palette_name).to_string();
            set_palette(Palette::from_name(&state.config.palette_name));
        }
    }
}

/// Writes the open tree to a plain-text file beside the secret file, reporting
/// the result in an alert.
fn export(state: &mut HomeState, format: ExportFormat) {
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
            Ok(out_path) => HomeModal::alert(format!("{}{}", texts::MSG_EXPORT_SUCCESS, out_path)),
            Err(e) => HomeModal::alert(format!("{e}")),
        },
    );
}

/// Drops `path` from the recent secret files, forgets it as the last opened file,
/// and persists the change.
fn delete_recent(state: &mut HomeState, path: &str) {
    state.config.recent_files.retain(|file| file != path);
    if state.config.last_opened_file.as_deref() == Some(path) {
        state.config.last_opened_file = None;
    }
    let _ = utils::save_config(&state.config);
}

/// Decrypts the file the unlock prompt was opened for and opens it in the Tree.
///
/// A wrong master password re-shows the prompt with the error, keeping what was
/// typed so the user can correct it.
fn unlock<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    modal: HomeModal,
) -> io::Result<()> {
    let HomeModal::Unlock(mut prompt) = modal else {
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
            prompt.reject(format!("{}{e}", texts::MSG_WRONG_PW));
            state.modal = Some(HomeModal::Unlock(prompt));
            Ok(())
        }
        Ok(node) => {
            let file_path = prompt.file_path().to_string();
            let passphrase = Zeroizing::new(passphrase.into_bytes());
            state.active_vault = Some(ActiveVaultState {
                path: Some(file_path.clone()),
                passphrase: passphrase.clone(),
                node: Some(node.clone()),
            });
            let _ = utils::push_recent_secret_file(&mut state.config, &file_path);
            open_tree(state, terminal, node, file_path, passphrase)
        }
    }
}

/// Creates the file the create dialog was opened for and opens it in the Tree.
///
/// A failed write re-shows the dialog with the error, keeping the typed pair.
fn create_secret<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    modal: HomeModal,
) -> io::Result<()> {
    let HomeModal::CreateSecret(mut prompt) = modal else {
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
            prompt.reject(format!("{e}"));
            state.modal = Some(HomeModal::CreateSecret(prompt));
            Ok(())
        }
        Ok(node) => {
            let passphrase = Zeroizing::new(passphrase.into_bytes());
            state.active_vault = Some(ActiveVaultState {
                path: Some(file_path.clone()),
                passphrase: passphrase.clone(),
                node: Some(node.clone()),
            });
            let _ = utils::push_recent_secret_file(&mut state.config, &file_path);
            open_tree(state, terminal, node, file_path, passphrase)
        }
    }
}

/// Re-encrypts the open secret file with the typed replacement master password.
///
/// Validation lives here, not in the prompt: the "current password" field is
/// checked against the open vault, which only this module holds. The order of the
/// three checks is the one users are used to — current password, then emptiness,
/// then a mismatched pair.
fn change_master_password<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    modal: HomeModal,
) -> io::Result<()> {
    let HomeModal::ChangePassword(mut prompt) = modal else {
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
        state.modal = Some(HomeModal::ChangePassword(prompt));
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
            prompt.reject_and_clear(format!("{}{e}", texts::MSG_SAVE_FAILED));
            state.modal = Some(HomeModal::ChangePassword(prompt));
            Ok(())
        }
        Ok(()) => {
            state.active_vault.as_mut().unwrap().passphrase = new_password.clone();
            open_tree(state, terminal, node, path, new_password)
        }
    }
}

/// Creates `file_path` with an empty master password and opens it in the Tree.
fn create_without_password<B: Backend>(
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
            state.modal = Some(HomeModal::alert(format!("{e}")));
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
            open_tree(state, terminal, node, file_path, passphrase)
        }
    }
}

/// Opens `node` in the Tree screen, autosaving it back to `path`.
///
/// Every Home flow that ends up in the Tree goes through here, so the tree is
/// always seeded from the same place.
pub fn open_tree<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    node: Node,
    path: String,
    passphrase: Zeroizing<Vec<u8>>,
) -> io::Result<()> {
    let mut tree_state = crate::tree::widget::TreeState::new(node);
    tree_state.display_full_path = state.config.display_full_path;
    tree_state.generator = state.generator;
    run_tree(
        state,
        terminal,
        tree_state,
        Some(SaveTarget { path, passphrase }),
    )
}
