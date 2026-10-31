//! The File Browser's rename input.

use crossterm::event::{KeyCode, KeyEvent};

use crate::file_browser::state::{FileBrowserAction, FileBrowserState, FocusTarget};
use crate::file_handling;

use super::text_edit::handle_text_edit;

pub(super) fn handle_rename_key(
    key: KeyEvent,
    state: &mut FileBrowserState,
) -> Option<FileBrowserAction> {
    if handle_text_edit(&mut state.rename_cursor, &mut state.rename_name, key.code) {
        return None;
    }

    match key.code {
        KeyCode::Esc => {
            state.focus = FocusTarget::List;
            state.rename_name.clear();
            state.rename_cursor = 0;
            state.rename_target.clear();
            None
        }
        KeyCode::Enter => {
            let new_name = state.rename_name.trim().to_string();
            if new_name.is_empty() {
                return None;
            }
            let old_target = state.rename_target.clone();
            let from = std::path::Path::new(&old_target);
            let to = state.current_dir.join(&new_name);
            match file_handling::rename_file(from, &to) {
                Ok(()) => {
                    let new_path_str = to.to_string_lossy().to_string();
                    let is_active = state.active_vault_path.as_deref() == Some(from);
                    if is_active {
                        state.active_vault_path = Some(to);
                    }
                    state.rename_target.clear();
                    state.focus = FocusTarget::List;
                    state.rename_name.clear();
                    state.rename_cursor = 0;
                    state.refresh_entries();
                    if is_active {
                        Some(FileBrowserAction::Renamed {
                            from: old_target,
                            to: new_path_str,
                        })
                    } else {
                        None
                    }
                }
                Err(e) => {
                    state.error = Some(format!("{e}"));
                    None
                }
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::super::fixtures::{key, make_state};
    use crate::file_browser::FileBrowserMode;
    use crate::file_browser::handle_key;
    use crate::file_browser::state::FocusTarget;

    // ── Esc in Rename mode cancels the rename, not the browser ──

    #[test]
    fn test_esc_in_rename_cancels_rename_not_browser() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.focus = FocusTarget::Rename;
        state.rename_name = "renamed.age".to_string();
        state.rename_cursor = state.rename_name.len();
        state.rename_target = "/tmp/original.age".to_string();

        let result = handle_key(key(KeyCode::Esc), &mut state);

        // The browser stays open; only the rename dialog is dismissed.
        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::List);
        // Edits are discarded.
        assert!(state.rename_name.is_empty());
        assert_eq!(state.rename_cursor, 0);
        assert!(state.rename_target.is_empty());
    }
}
