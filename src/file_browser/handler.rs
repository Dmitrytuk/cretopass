use crossterm::event::{KeyCode, KeyEvent};

use super::state::{FileBrowserAction, FileBrowserMode, FileBrowserState, FileFilter, FocusTarget};
use crate::file_handling;
use crate::theming::texts;

/// Processes a key event and returns an action if the user completed
/// or cancelled the file browser.  Returns `None` when the browser
/// state was just mutated (re-render needed).
pub fn handle_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    // ── Delete confirmation mode ──
    if state.confirm_delete.is_some() {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(ref path_str) = state.confirm_delete.clone() {
                    let path = std::path::Path::new(path_str);
                    if Some(path) == state.active_file_path.as_deref() {
                        state.error = Some(texts::MSG_CANNOT_DELETE_FILE.to_string());
                    } else if file_handling::delete_file(path).is_ok() {
                        state.refresh_entries();
                    }
                }
                state.confirm_delete = None;
                None
            }
            _ => {
                state.confirm_delete = None;
                None
            }
        };
    }

    // ── Search-mode keys ──
    if state.search_active {
        match key.code {
            KeyCode::Esc => {
                state.search_query.clear();
                state.search_active = false;
                state.clamp_selection();
                return None;
            }
            KeyCode::Enter => {
                state.search_active = false;
                return None;
            }
            KeyCode::Char(c) => {
                state.search_query.push(c);
                state.clamp_selection();
                return None;
            }
            KeyCode::Backspace => {
                state.search_query.pop();
                state.clamp_selection();
                return None;
            }
            KeyCode::Delete => {
                state.search_query.pop();
                state.clamp_selection();
                return None;
            }
            _ => {} // fall through to list navigation during search
        }
    }

    // ── Global keys ──
    match key.code {
        KeyCode::Esc => return Some(FileBrowserAction::Cancelled),
        KeyCode::Char('/') if !state.search_active && state.focus == FocusTarget::List => {
            state.search_active = true;
            return None;
        }
        _ => {}
    }

    // ── Focus-specific keys ──
    match state.focus {
        FocusTarget::List => handle_list_key(key, state),
        FocusTarget::Input => handle_input_key(key, state),
        FocusTarget::Rename => handle_rename_key(key, state),
    }
}

fn handle_list_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    let indices = state.filtered_indices();

    if indices.is_empty() {
        return match key.code {
            KeyCode::Esc => Some(FileBrowserAction::Cancelled),
            KeyCode::Char('h') | KeyCode::Left => {
                go_to_parent(state);
                None
            }
            KeyCode::Char('f') => {
                state.filter = match state.filter {
                    FileFilter::All => FileFilter::PwdKeeperOnly,
                    FileFilter::PwdKeeperOnly => FileFilter::All,
                };
                None
            }
            KeyCode::Char('N') if state.mode == FileBrowserMode::CreateFile => {
                state.focus = FocusTarget::Input;
                None
            }
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
            if let Some(entry) = state.entries.get(idx) {
                if entry.is_dir {
                    match file_handling::list_dir(&entry.path) {
                        Ok(new_entries) => {
                            state.current_dir = entry.path.clone();
                            state.entries = new_entries;
                            state.selected = 0;
                            state.search_active = false;
                            state.search_query.clear();
                        }
                        Err(e) => state.error = Some(format!("{e}")),
                    }
                } else if state.mode == FileBrowserMode::OpenFile
                    || state.mode == FileBrowserMode::ImportFile
                {
                    return Some(FileBrowserAction::Selected(
                        entry.path.to_string_lossy().to_string(),
                    ));
                }
            }
            None
        }
        KeyCode::Char('h') | KeyCode::Left => {
            go_to_parent(state);
            None
        }
        KeyCode::Char('f') => {
            state.filter = match state.filter {
                FileFilter::All => FileFilter::PwdKeeperOnly,
                FileFilter::PwdKeeperOnly => FileFilter::All,
            };
            state.clamp_selection();
            None
        }
        KeyCode::Char('n') if state.mode == FileBrowserMode::CreateFile => {
            create_new_folder(state);
            None
        }
        KeyCode::Char('N') if state.mode == FileBrowserMode::CreateFile => {
            state.focus = FocusTarget::Input;
            None
        }
        KeyCode::Char('r') => {
            if let Some(entry) = state.entries.get(state.selected) {
                if entry.is_age_file && !entry.is_dir && state.mode != FileBrowserMode::ImportFile {
                    state.focus = FocusTarget::Rename;
                    state.rename_name = entry.name.clone();
                    state.rename_cursor = entry.name.len();
                    state.rename_target = entry.path.to_string_lossy().to_string();
                }
            }
            None
        }
        KeyCode::Char('d') | KeyCode::Delete => {
            if let Some(entry) = state.entries.get(state.selected) {
                if entry.is_age_file && !entry.is_dir && state.mode != FileBrowserMode::ImportFile {
                    state.confirm_delete = Some(entry.path.to_string_lossy().to_string());
                }
            }
            None
        }
        _ => None,
    }
}

/// Handles common text-editing key events (Char, Backspace, Delete, Left,
/// Right, Home, End).  Returns `true` if the key was consumed by editing.
fn handle_text_edit(cursor: &mut usize, text: &mut String, key: KeyCode) -> bool {
    match key {
        KeyCode::Char(c) => {
            crate::cursor::insert_char_at(text, cursor, c);
            true
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                *cursor = crate::cursor::backspace_at(text, *cursor);
            }
            true
        }
        KeyCode::Delete => {
            crate::cursor::delete_at(text, *cursor);
            true
        }
        KeyCode::Left => {
            *cursor = crate::cursor::cursor_left(text, *cursor);
            true
        }
        KeyCode::Right => {
            *cursor = crate::cursor::cursor_right(text, *cursor);
            true
        }
        KeyCode::Home => {
            *cursor = 0;
            true
        }
        KeyCode::End => {
            *cursor = text.len();
            true
        }
        _ => false,
    }
}

fn handle_input_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    // Check focus-switch key before general Char(c) to avoid unreachable pattern
    if key.code == KeyCode::Char('N') || key.code == KeyCode::Esc {
        state.focus = FocusTarget::List;
        return None;
    }

    if handle_text_edit(
        &mut state.creating_cursor,
        &mut state.creating_name,
        key.code,
    ) {
        return None;
    }

    match key.code {
        KeyCode::Enter => {
            let name = state.creating_name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let full_path = state.current_dir.join(&name);
            Some(FileBrowserAction::Selected(
                full_path.to_string_lossy().to_string(),
            ))
        }
        _ => None,
    }
}

fn handle_rename_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
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
                    let is_active = state.active_file_path.as_deref() == Some(from);
                    if is_active {
                        state.active_file_path = Some(to);
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

fn go_to_parent(state: &mut FileBrowserState) {
    let parent = match state.current_dir.parent() {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    if parent == state.current_dir {
        return;
    }

    let prev_dir = state.current_dir.clone();
    match file_handling::list_dir(&parent) {
        Ok(new_entries) => {
            let prev_name = prev_dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string());

            state.current_dir = parent;
            state.entries = new_entries;
            state.search_active = false;
            state.search_query.clear();

            state.selected = prev_name
                .and_then(|name| state.entries.iter().position(|e| e.name == name))
                .unwrap_or(0);
            state.clamp_selection();
        }
        Err(e) => state.error = Some(format!("{e}")),
    }
}

fn create_new_folder(state: &mut FileBrowserState) {
    let mut name = texts::NEW_FOLDER_DEFAULT.to_string();
    let mut counter = 1;
    while state.current_dir.join(&name).exists() {
        counter += 1;
        name = format!("{} ({})", texts::NEW_FOLDER_DEFAULT, counter);
    }

    let new_path = state.current_dir.join(&name);
    match file_handling::create_dir(&new_path) {
        Ok(()) => {
            if let Ok(new_entries) = file_handling::list_dir(&state.current_dir) {
                state.entries = new_entries;
                state.selected = state
                    .entries
                    .iter()
                    .position(|e| e.name == name)
                    .unwrap_or(0);
                state.clamp_selection();
            }
        }
        Err(e) => state.error = Some(format!("{e}")),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::*;
    use crate::file_browser::FileBrowserMode;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_state(mode: FileBrowserMode) -> FileBrowserState {
        let mut state = FileBrowserState::new(mode);
        // Point to /tmp so list_dir works and entries are populated
        state.current_dir = std::path::PathBuf::from("/tmp");
        state.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap_or_default();
        state.selected = 0;
        state
    }

    // ── N switches focus in CreateFile mode ──

    #[test]
    fn test_n_switches_to_input_create_file() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        assert_eq!(state.focus, FocusTarget::List);

        let result = handle_key(key(KeyCode::Char('N')), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::Input);
    }

    #[test]
    fn test_n_switches_to_input_empty_list() {
        let mut state = FileBrowserState::new(FileBrowserMode::CreateFile);
        state.current_dir = std::path::PathBuf::from("/tmp/__empty_test_dir__");
        let _ = std::fs::create_dir_all(&state.current_dir);
        state.entries = file_handling::list_dir(&state.current_dir).unwrap();
        assert_eq!(state.filtered_len(), 0);

        let result = handle_key(key(KeyCode::Char('N')), &mut state);
        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::Input);

        let _ = std::fs::remove_dir(&state.current_dir);
    }

    #[test]
    fn test_n_noop_in_open_file_mode() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        assert_eq!(state.focus, FocusTarget::List);

        handle_key(key(KeyCode::Char('N')), &mut state);

        // Focus should remain List (N only works in CreateFile mode)
        assert_eq!(state.focus, FocusTarget::List);
    }

    // ── Enter in Input mode ──

    #[test]
    fn test_enter_in_input_returns_selected() {
        let mut state = make_state(FileBrowserMode::CreateFile);
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
    }

    #[test]
    fn test_enter_in_input_empty_name_noop() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "   ".to_string(); // whitespace → trimmed to empty

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None);
    }

    // ── Full create-file flow ──

    #[test]
    fn test_full_flow_create_file() {
        let mut state = FileBrowserState::new(FileBrowserMode::CreateFile);
        state.current_dir = std::path::PathBuf::from("/tmp");
        state.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap_or_default();

        // Step 1: N → focus becomes Input
        handle_key(key(KeyCode::Char('N')), &mut state);
        assert_eq!(state.focus, FocusTarget::Input);

        // Step 2: type filename
        for c in "myvault.age".chars() {
            handle_key(key(KeyCode::Char(c)), &mut state);
        }
        assert_eq!(state.creating_name, "myvault.age");

        // Step 3: Enter → Selected returned
        let result = handle_key(key(KeyCode::Enter), &mut state);
        assert!(result.is_some());
        match result.unwrap() {
            FileBrowserAction::Selected(path) => {
                assert!(
                    path.contains("myvault.age"),
                    "path should contain filename: {path}"
                );
                assert!(path.contains("/tmp"), "path should contain /tmp: {path}");
            }
            FileBrowserAction::Cancelled => panic!("expected Selected, got Cancelled"),
            FileBrowserAction::Renamed { .. } => panic!("expected Selected, got Renamed"),
        }
    }

    // ── N in Input mode returns to List ──

    #[test]
    fn test_n_in_input_returns_to_list() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;

        let result = handle_key(key(KeyCode::Char('N')), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::List);
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
    fn test_backspace_in_input_removes_chars() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "abc".to_string();
        state.creating_cursor = 3;

        handle_key(key(KeyCode::Backspace), &mut state);

        assert_eq!(state.creating_name, "ab");
        assert_eq!(state.creating_cursor, 2);
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
