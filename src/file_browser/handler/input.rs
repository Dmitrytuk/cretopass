//! The File Browser's name input: typing, editing and committing the typed name.

use crossterm::event::{KeyCode, KeyEvent};

use crate::cursor::insert_str_at;
use crate::file_browser::state::{FileBrowserAction, FileBrowserState, FocusTarget};
use crate::theming::texts;

use super::destination::begin_destination;
use super::list::create_dir_named;
use super::text_edit::handle_text_edit;

/// Inserts the already-sanitised `text` into the name input at its cursor.
pub(super) fn handle_input_paste(state: &mut FileBrowserState, text: &str) {
    insert_str_at(&mut state.creating_name, &mut state.creating_cursor, text);
}

pub(super) fn handle_input_key(
    key: KeyEvent,
    state: &mut FileBrowserState,
) -> Option<FileBrowserAction> {
    if handle_text_edit(
        &mut state.creating_cursor,
        &mut state.creating_name,
        key.code,
    ) {
        return None;
    }

    match key.code {
        KeyCode::Enter => {
            // A share names a folder rather than a file, so Enter commits it here.
            if state.mode.is_share_destination() {
                return begin_destination(state);
            }
            // In export mode Enter accepts the draft and returns to the list; the
            // write runs from there with `e`, which is also where the empty-name
            // check lives.
            if state.mode.export_format().is_some() {
                state.focus = FocusTarget::List;
                return None;
            }
            let name = state.creating_name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let full_path = state.current_dir.join(&name);
            if full_path.exists() {
                state.error = Some(texts::MSG_FILE_EXISTS.to_string());
                return None;
            }
            if state.creating_is_dir {
                create_dir_named(state, &name);
                state.creating_is_dir = false;
                state.creating_name.clear();
                state.creating_cursor = 0;
                state.focus = FocusTarget::List;
                return None;
            }
            Some(FileBrowserAction::Selected(
                full_path.to_string_lossy().to_string(),
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::super::fixtures::{key, make_state};
    use crate::file_browser::handle_key;
    use crate::file_browser::state::{AddPrefix, FocusTarget};
    use crate::file_browser::{FileBrowserAction, FileBrowserMode};
    use crate::theming::texts;

    // ── Enter in Input mode ──

    #[test]
    fn test_enter_in_input_returns_selected() {
        let dir = std::path::PathBuf::from("/tmp/__test_create_selected__");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut state = make_state(FileBrowserMode::CreateFile);
        state.current_dir = dir.clone();
        state.focus = FocusTarget::Input;
        state.creating_name = "test_create_file.age".to_string();

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert!(result.is_some());
        match result.unwrap() {
            FileBrowserAction::Selected(path) => {
                assert!(
                    path.contains("test_create_file.age"),
                    "path should contain filename: {path}"
                );
            }
            FileBrowserAction::Cancelled => panic!("expected Selected, got Cancelled"),
            FileBrowserAction::Renamed { .. } => panic!("expected Selected, got Renamed"),
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_enter_in_input_existing_name_blocks() {
        let dir = std::path::PathBuf::from("/tmp/__test_create_exists__");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("taken.age"), b"x").unwrap();

        let mut state = make_state(FileBrowserMode::CreateFile);
        state.current_dir = dir.clone();
        state.focus = FocusTarget::Input;
        state.creating_name = "taken.age".to_string();

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None, "existing file must not be selected");
        assert_eq!(state.error.as_deref(), Some(texts::MSG_FILE_EXISTS));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_enter_in_input_empty_name_noop() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "   ".to_string(); // whitespace → trimmed to empty

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None);
    }
    // ── Esc in Input mode returns to List ──

    #[test]
    fn test_esc_in_input_returns_to_list() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "draft".to_string();

        let result = handle_key(key(KeyCode::Esc), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::List);
        // The draft is kept so the user can resume.
        assert_eq!(state.creating_name, "draft");
    }
    // ── Typing in Input mode ──

    #[test]
    fn test_typing_in_input_insers_chars() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;

        handle_key(key(KeyCode::Char('h')), &mut state);
        handle_key(key(KeyCode::Char('i')), &mut state);

        assert_eq!(state.creating_name, "hi");
        assert_eq!(state.creating_cursor, 2);
    }

    #[test]
    fn test_typing_prefix_chars_in_input_is_text() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;

        // `a` / `f` are typed as text while the name input is focused.
        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('f')), &mut state);

        assert_eq!(state.creating_name, "af");
        assert_eq!(state.pending, AddPrefix::None);
    }
    // ── Navigate in Input mode ──

    #[test]
    fn test_cursor_left_right_in_input() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "abcd".to_string();
        state.creating_cursor = 4;

        handle_key(key(KeyCode::Left), &mut state);
        assert_eq!(state.creating_cursor, 3);

        handle_key(key(KeyCode::Right), &mut state);
        assert_eq!(state.creating_cursor, 4);

        // Home / End
        handle_key(key(KeyCode::Home), &mut state);
        assert_eq!(state.creating_cursor, 0);

        handle_key(key(KeyCode::End), &mut state);
        assert_eq!(state.creating_cursor, 4);
    }
}
