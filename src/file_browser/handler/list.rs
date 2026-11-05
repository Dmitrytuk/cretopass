//! The File Browser's list keys: navigation, entering directories, the `a` add
//! prefix and the `r`/`d` file actions.

use crossterm::event::{KeyCode, KeyEvent};

use crate::file_browser::state::{
    AddPrefix, FileBrowserAction, FileBrowserMode, FileBrowserState, FocusTarget,
};
use crate::file_handling;

use super::destination::begin_destination;

pub(super) fn handle_list_key(
    key: KeyEvent,
    state: &mut FileBrowserState,
) -> Option<FileBrowserAction> {
    let indices = state.filtered_indices();

    if indices.is_empty() {
        return match key.code {
            KeyCode::Esc => Some(FileBrowserAction::Cancelled),
            KeyCode::Char('h') | KeyCode::Left => {
                go_to_parent(state);
                None
            }
            KeyCode::Char('a') if state.mode == FileBrowserMode::CreateFile => {
                state.pending = AddPrefix::Pending;
                None
            }
            // The name and the write are available even with nothing to list.
            KeyCode::Char('n') if state.mode.names_destination() => {
                state.focus = FocusTarget::Input;
                None
            }
            KeyCode::Char('e') if state.mode.names_destination() => begin_destination(state),
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let pos = indices
                .iter()
                .position(|&i| i == state.selected)
                .unwrap_or(0);
            let next_pos = (pos + 1) % indices.len();
            state.selected = indices[next_pos];
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let pos = indices
                .iter()
                .position(|&i| i == state.selected)
                .unwrap_or(0);
            let prev_pos = if pos == 0 { indices.len() - 1 } else { pos - 1 };
            state.selected = indices[prev_pos];
            None
        }
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
            let idx = state.selected;
            let is_dir = state.entries.get(idx).is_some_and(|entry| entry.is_dir);

            if is_dir {
                let path = state.entries.get(idx).map(|entry| entry.path.clone())?;
                // Entering a directory only requests the listing; the event loop
                // applies it, so a slow directory cannot freeze the UI.
                state.navigate_to(path, None);
                return None;
            }

            if state.mode.names_destination() {
                // The list picks the directory only: a file row is not a target.
                // The export name is edited with `n` and written with `e`; the
                // share folder is edited with `n` and committed with `e` on the
                // list or Enter in the input.
                return None;
            }

            if matches!(
                state.mode,
                FileBrowserMode::OpenFile | FileBrowserMode::ImportFile
            ) {
                let entry = state.entries.get(idx)?;
                return Some(FileBrowserAction::Selected(
                    entry.path.to_string_lossy().to_string(),
                ));
            }

            None
        }
        KeyCode::Char('h') | KeyCode::Left => {
            go_to_parent(state);
            None
        }
        KeyCode::Char('a') if state.mode == FileBrowserMode::CreateFile => {
            state.pending = AddPrefix::Pending;
            None
        }
        // Naming a destination: `n` edits the name; `e` writes an export to the
        // directory now open or commits a share's folder, and Enter in the input
        // also commits a share.
        KeyCode::Char('n') if state.mode.names_destination() => {
            state.focus = FocusTarget::Input;
            None
        }
        KeyCode::Char('e') if state.mode.names_destination() => begin_destination(state),
        KeyCode::Char('r') => {
            // Any non-directory row may be renamed: the name is not an age
            // header check (browsing reads no contents).
            if let Some(entry) = state.entries.get(state.selected)
                && !entry.is_dir
                && state.mode.allows_entry_actions()
            {
                state.focus = FocusTarget::Rename;
                state.rename_name = entry.name.clone();
                state.rename_cursor = entry.name.len();
                state.rename_target = entry.path.to_string_lossy().to_string();
            }
            None
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(entry) = state.entries.get(state.selected)
                && !entry.is_dir
                && state.mode.allows_entry_actions()
            {
                state.confirm_delete = Some(entry.path.to_string_lossy().to_string());
            }
            None
        }
        _ => None,
    }
}
fn go_to_parent(state: &mut FileBrowserState) {
    let parent = match state.current_dir.parent() {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    if parent == state.current_dir {
        return;
    }

    // Reselect the directory we came from once the parent listing lands.
    let prev_name = state
        .current_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string());
    state.navigate_to(parent, prev_name);
}
/// Creates a directory named `name` in the current directory, refreshes the
/// listing, and selects the new entry.  Reports I/O errors through the error
/// popup.
pub(super) fn create_dir_named(state: &mut FileBrowserState, name: &str) {
    let new_path = state.current_dir.join(name);
    match file_handling::create_dir(&new_path) {
        Ok(()) => {
            // Refresh in the background, selecting the folder just created.
            let dir = state.current_dir.clone();
            state.navigate_to(dir, Some(name.to_string()));
        }
        Err(e) => state.error = Some(format!("{e}")),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::super::fixtures::{key, make_state};
    use crate::file_browser::FileBrowserAction;
    use crate::file_browser::FileBrowserMode;
    use crate::file_browser::handle_key;
    use crate::file_browser::state::{AddPrefix, FocusTarget};

    #[test]
    fn test_esc_closes_immediately_while_a_listing_is_pending() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.navigate_to(std::path::PathBuf::from("/somewhere"), None);
        assert!(state.is_loading());

        let result = handle_key(key(KeyCode::Esc), &mut state);

        assert_eq!(
            result,
            Some(FileBrowserAction::Cancelled),
            "Esc must not wait for a stuck listing"
        );
    }

    #[test]
    fn test_a_arms_no_prefix_in_open_file_mode() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        assert_eq!(state.focus, FocusTarget::List);

        handle_key(key(KeyCode::Char('a')), &mut state);

        // `a` only works in CreateFile mode.
        assert_eq!(state.pending, AddPrefix::None);
        assert_eq!(state.focus, FocusTarget::List);
    }
}
