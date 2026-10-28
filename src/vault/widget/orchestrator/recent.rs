//! The recent secret files: opening one, renaming one, dropping one.
//!
//! The list lives in the configuration, so every change here is persisted.

use std::io;

use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::file_handling;
use crate::vault::utils;

use super::super::modal::UnlockPrompt;
use super::super::{HomeState, Modal};
use super::errors;
use super::vault::launch_tree;
/// Drops `path` from the recent secret files, forgets it as the last opened file,
/// and persists the change.
pub(super) fn delete_recent(state: &mut HomeState, path: &str) {
    state.config.recent_files.retain(|file| file != path);
    if state.config.last_opened_file.as_deref() == Some(path) {
        state.config.last_opened_file = None;
    }
    let _ = utils::save_config(&state.config);
}

/// Opens a recent file: the already-decrypted one straight away, any other after
/// checking it is a secret file and asking for its master password.
pub(super) fn open_recent<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    path: String,
) -> io::Result<()> {
    let is_open = state.active_vault.as_ref().is_some_and(|active| {
        active.path.as_deref() == Some(path.as_str()) && active.node.is_some()
    });
    if is_open {
        return launch_tree(state, terminal);
    }

    state.modal = Some(match file_handling::validate_encrypted_file(&path) {
        Ok(data) => Modal::Unlock(UnlockPrompt::new(path, data)),
        Err(e) => Modal::alert(errors::message(&e)),
    });
    Ok(())
}

/// Takes `from`'s place with `to` everywhere the app remembers the old path: the
/// open vault, the last-opened file, and the recent list.
pub(super) fn rename_file(state: &mut HomeState, from: &str, to: &str) {
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
