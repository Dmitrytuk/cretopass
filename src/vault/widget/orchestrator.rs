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

use super::modal::{
    ChangePasswordPrompt, Confirmation, CreateSecretPrompt, HomeAction, Setting, UnlockPrompt,
};
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
    // The File Browser remembers where it was for its next opening, whatever the
    // action turns out to be. Every key it handles ends up here, so capturing it
    // once is enough — the last action before it leaves the screen keeps the
    // directory the user was looking at.
    if let HomeModal::FileBrowser(browser) = &modal {
        state.browser_session = browser.session();
    }

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
        HomeAction::RenameFile { from, to } => {
            rename_file(state, &from, &to);
            // The user stays in the browser to rename something else.
            state.modal = Some(modal);
            Ok(())
        }
        HomeAction::ChoseFileToImport { import_path } => import(state, terminal, import_path),
        HomeAction::ChoseFileToCreate { file_path } => {
            choose_file_to_create(state, file_path);
            Ok(())
        }
        HomeAction::ChoseFileToOpen { file_path } => {
            choose_file_to_open(state, file_path);
            Ok(())
        }
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

/// Takes `from`'s place with `to` everywhere the app remembers the old path: the
/// open vault, the last-opened file, and the recent list.
fn rename_file(state: &mut HomeState, from: &str, to: &str) {
    if state
        .active_vault
        .as_ref()
        .is_some_and(|active| active.path.as_deref() == Some(from))
    {
        state.active_vault.as_mut().unwrap().path = Some(to.to_string());
    }
    if state.config.last_opened_file.as_deref() == Some(from) {
        state.config.last_opened_file = Some(to.to_string());
    }
    for path in &mut state.config.recent_files {
        if path == from {
            *path = to.to_string();
            break;
        }
    }
    let _ = utils::save_config(&state.config);
}

/// Merges the tree stored in `import_path` into the open vault and saves it.
///
/// The imported tree is filed under a timestamped wrapper node inside the open
/// Root, so one import never collides with the next.
fn import<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    import_path: String,
) -> io::Result<()> {
    let imported = match file_handling::import::import_tree(&import_path) {
        Ok(node) => node,
        Err(e) => {
            state.modal = Some(HomeModal::alert(format!("{e}")));
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
        Ok(()) => HomeModal::alert(texts::MSG_IMPORT_SUCCESS.to_string()),
        Err(e) => HomeModal::alert(format!("{}{}", texts::MSG_SAVE_FAILED, e)),
    });
    Ok(())
}

/// Files `imported` under a timestamped wrapper inside the open Root, returning
/// what to save.
///
/// Returns `None` after showing an alert when there is nothing to import into,
/// or nothing to save to.
fn wrap_import(
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
        state.modal = Some(HomeModal::alert(message.to_string()));
    }
    target
}

/// The wrapper node an import is filed under: the imported file's stem and the
/// local time, so repeated imports of the same file stay apart.
fn wrapper_name(import_path: &str) -> String {
    let stem = std::path::Path::new(import_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_string())
        .unwrap_or_else(|| texts::IMPORT_NAME_FALLBACK.to_string());
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S");
    format!("imported_{stem}_{stamp}")
}

/// The user picked where a new secret file should go: ask for its master password,
/// or report why it cannot be created there.
fn choose_file_to_create(state: &mut HomeState, file_path: String) {
    if std::path::Path::new(&file_path).exists() {
        state.modal = Some(HomeModal::alert(texts::MSG_FILE_EXISTS.to_string()));
    } else if file_handling::probe_path_writable(&file_path).is_ok() {
        state.modal = Some(HomeModal::CreateSecret(CreateSecretPrompt::new(file_path)));
    } else {
        state.modal = Some(HomeModal::alert(texts::MSG_CANNOT_WRITE.to_string()));
    }
}

/// The user picked a file to open: ask for its master password, or report why it
/// is not a secret file.
fn choose_file_to_open(state: &mut HomeState, file_path: String) {
    state.modal = Some(match file_handling::validate_encrypted_file(&file_path) {
        Ok(data) => HomeModal::Unlock(UnlockPrompt::new(file_path, data)),
        Err(e) => HomeModal::alert(format!("{e}")),
    });
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
