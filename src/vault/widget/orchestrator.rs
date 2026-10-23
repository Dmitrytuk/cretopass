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
use crate::tree::app_tree::SaveTarget;
use crate::tree::node::Node;
use crate::vault::active_file::ActiveVaultState;
use crate::vault::utils;

use super::modal::HomeAction;
use super::secret_field::SecretFieldGroup;
use super::{HomeModal, HomeState, run_tree, run_with_spinner};

/// Performs `action`. The modal that produced it has already been removed from
/// [`HomeState`], so "close" means leaving it that way and any other transition
/// installs the modal it names.
pub fn execute<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    action: HomeAction,
) -> io::Result<()> {
    match action {
        HomeAction::Close => Ok(()),
        HomeAction::DeleteRecent { path } => {
            delete_recent(state, &path);
            Ok(())
        }
        HomeAction::ChangePasswordPrompt => {
            state.modal = Some(HomeModal::ChangePwInput {
                fields: SecretFieldGroup::with_fields(3),
                error: None,
            });
            Ok(())
        }
        HomeAction::ReopenCreateSecret { file_path } => {
            state.modal = Some(HomeModal::CreateSecretFile {
                file_path,
                fields: SecretFieldGroup::with_fields(2),
                error: None,
            });
            Ok(())
        }
        HomeAction::CreateWithoutPassword { file_path } => {
            create_without_password(state, terminal, file_path)
        }
        HomeAction::Export { format } => {
            export(state, format);
            Ok(())
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
