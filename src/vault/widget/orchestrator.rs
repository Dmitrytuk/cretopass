//! The one place that turns a modal's [`HomeAction`] into real work.
//!
//! Everything the Home screen asks for lands here: disk I/O, the blocking spinner,
//! configuration writes, screen transitions and launching the Tree. The modals stay
//! pure input (see [`crate::vault::widget::modal`]); this module is allowed to be
//! shallow, because all it does is perform intentions. It is the facade over one
//! sub-module per flow.

use std::io;
use std::path::{Path, PathBuf};

use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::file_browser::FileBrowserMode;
use crate::file_handling::export::ExportFormat;
use crate::theming::texts;
use crate::theming::{Palette, set_palette};
use crate::vault::utils;

mod errors;
mod export;
mod import;
mod recent;
mod settings;
mod share;
mod vault;

use export::export;
use import::import;
use recent::{delete_recent, open_recent, rename_file};
use settings::apply_setting;
use share::share;
use vault::{
    change_master_password, choose_file_to_create, choose_file_to_open, create_secret,
    create_without_password, launch_tree, unlock,
};

use super::modal::{
    About, BrowserModal, ChangePasswordPrompt, ConfigurationModal, Confirmation,
    CreateSecretPrompt, ExportWizard, HomeAction, SharePassphrase,
};
use super::{HomeState, Modal};

/// Performs `action`. `modal` is the modal that produced it, if it came from one:
/// the Home menu is the screen itself, so it passes `None` and has nothing to put
/// back on screen. "Close" leaves the screen without a modal, and a transition
/// installs the modal it names.
pub fn execute<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    action: HomeAction,
    modal: Option<Modal>,
) -> io::Result<()> {
    // The File Browser remembers where it was for its next opening, whatever the
    // action turns out to be. Every key it handles ends up here, so capturing it
    // once is enough — the last action before it leaves the screen keeps the
    // directory the user was looking at.
    if let Some(Modal::FileBrowser(browser)) = &modal {
        state.browser_session = browser.session();
    }

    match action {
        HomeAction::Close => Ok(()),
        HomeAction::DeleteRecent { path } => {
            delete_recent(state, &path);
            Ok(())
        }
        HomeAction::ChangePasswordPrompt => {
            state.modal = Some(Modal::ChangePassword(ChangePasswordPrompt::new()));
            Ok(())
        }
        HomeAction::ReopenCreateSecret { file_path } => {
            state.modal = Some(Modal::CreateSecret(CreateSecretPrompt::new(file_path)));
            Ok(())
        }
        HomeAction::ConfirmNoPassphrase { file_path } => {
            state.modal = Some(Modal::Confirmation(Confirmation::no_passphrase(file_path)));
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
            // The user stays in the prompt to rename something else.
            state.modal = modal;
            Ok(())
        }
        HomeAction::ImportFileChosen { import_path } => import(state, terminal, import_path),
        HomeAction::CreateFileChosen { file_path } => {
            choose_file_to_create(state, file_path);
            Ok(())
        }
        HomeAction::OpenFileChosen { file_path } => {
            choose_file_to_open(state, file_path);
            Ok(())
        }
        HomeAction::Quit => {
            state.quit = true;
            Ok(())
        }
        HomeAction::Back => {
            if state
                .active_vault
                .as_ref()
                .is_some_and(|active| active.node.is_some())
            {
                launch_tree(state, terminal)
            } else {
                state.quit = true;
                Ok(())
            }
        }
        HomeAction::LaunchTree => launch_tree(state, terminal),
        HomeAction::ChooseFileToOpen => {
            open_browser(state, FileBrowserMode::OpenFile);
            Ok(())
        }
        HomeAction::ChooseFileToCreate => {
            open_browser(state, FileBrowserMode::CreateFile);
            Ok(())
        }
        HomeAction::ChooseFileToImport => {
            open_browser(state, FileBrowserMode::ImportFile);
            Ok(())
        }
        HomeAction::OpenAbout => {
            state.modal = Some(Modal::About(About));
            Ok(())
        }
        HomeAction::OpenConfiguration => {
            state.modal = Some(Modal::Configuration(ConfigurationModal::new()));
            Ok(())
        }
        HomeAction::BeginChangePassword => {
            state.modal = Some(if state.active_vault.is_some() {
                Modal::Confirmation(Confirmation::change_password())
            } else {
                Modal::error(texts::MSG_NO_OPEN_FILE.to_string())
            });
            Ok(())
        }
        HomeAction::BeginExport => {
            state.modal = Some(if state.active_vault.is_some() {
                Modal::Export(ExportWizard::new())
            } else {
                Modal::error(texts::MSG_NO_OPEN_FILE.to_string())
            });
            Ok(())
        }
        HomeAction::BeginExportDestination { format } => {
            open_export_destination(state, format);
            Ok(())
        }
        HomeAction::BeginShare => {
            open_share_destination(state);
            Ok(())
        }
        HomeAction::ShareDestinationChosen { dest } => {
            state.modal = Some(Modal::SharePassphrase(SharePassphrase::new(dest)));
            Ok(())
        }
        HomeAction::ShareNoPassphrase { dest } => {
            state.modal = Some(Modal::Confirmation(Confirmation::share_no_passphrase(dest)));
            Ok(())
        }
        HomeAction::ReopenSharePassphrase { dest } => {
            state.modal = Some(Modal::SharePassphrase(SharePassphrase::new(dest)));
            Ok(())
        }
        HomeAction::OpenRecent { path } => open_recent(state, terminal, path),
        HomeAction::RequestDeleteRecent { path } => {
            let is_open = state
                .active_vault
                .as_ref()
                .and_then(|active| active.path.as_deref())
                == Some(path.as_str());

            state.modal = Some(if is_open {
                Modal::error(texts::MSG_CANNOT_DELETE_OPEN.to_string())
            } else {
                Modal::Confirmation(Confirmation::delete_recent(path))
            });
            Ok(())
        }
        HomeAction::Export { path, format } => {
            export(state, &path, format);
            Ok(())
        }
        HomeAction::Share { dest, passphrase } => share(state, terminal, &dest, &passphrase),
        HomeAction::ToggleSetting(setting) => {
            apply_setting(state, setting);
            // The dialog stays open so the user sees the setting take effect.
            state.modal = modal;
            Ok(())
        }
        HomeAction::SaveConfig => {
            let _ = utils::save_config(&state.config);
            state.modal = Some(Modal::success(texts::MSG_CONFIG_SAVED.to_string()));
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

/// Opens the File Browser to name the destination of an export of the open vault.
///
/// Without an open tree there is no directory, stem, or content to export, so the
/// user gets the "no open file" alert instead of a browser that cannot finish.
fn open_export_destination(state: &mut HomeState, format: ExportFormat) {
    let browser = state.active_vault.as_ref().and_then(|active| {
        let path = active.path.as_deref()?;
        active.node.as_ref()?;
        Some(BrowserModal::for_export(
            format,
            Some(Path::new(path)),
            None,
            &state.browser_session,
        ))
    });

    state.modal = Some(match browser {
        Some(browser) => Modal::FileBrowser(Box::new(browser)),
        None => Modal::error(texts::MSG_NO_OPEN_FILE.to_string()),
    });
}

/// Opens the File Browser to name the folder a share bundle is written into.
///
/// Without an open tree there is no directory, stem, or content to share, so the
/// user gets the "no open file" alert instead of a browser that cannot finish.
fn open_share_destination(state: &mut HomeState) {
    let browser = state.active_vault.as_ref().and_then(|active| {
        let path = active.path.as_deref()?;
        active.node.as_ref()?;
        Some(BrowserModal::for_share(
            Some(Path::new(path)),
            None,
            &state.browser_session,
        ))
    });

    state.modal = Some(match browser {
        Some(browser) => Modal::FileBrowser(Box::new(browser)),
        None => Modal::error(texts::MSG_NO_OPEN_FILE.to_string()),
    });
}

/// Opens the File Browser, seeded with the session memory and protecting the open
/// vault from being overwritten or deleted.
fn open_browser(state: &mut HomeState, mode: FileBrowserMode) {
    let mut browser = BrowserModal::new(mode, &state.browser_session);
    browser.set_active_vault_path(
        state
            .active_vault
            .as_ref()
            .and_then(|active| active.path.as_ref().map(PathBuf::from)),
    );
    state.modal = Some(Modal::FileBrowser(Box::new(browser)));
}
