use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::state::{
    AddPrefix, FileBrowserAction, FileBrowserMode, FileBrowserState, FileFilter, FocusTarget,
    build_locations, list_entries, preselect_location,
};
use crate::file_handling;
use crate::theming::texts;

/// Processes a key event and returns an action if the user completed
/// or cancelled the file browser.  Returns `None` when the browser
/// state was just mutated (re-render needed).
pub fn handle_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    // ── Error popup: any key dismisses it and returns to the dialog below ──
    if state.error.is_some() {
        state.error = None;
        return None;
    }

    // ── Add file / add directory prefix (`a`) in CreateFile mode ──
    if state.pending == AddPrefix::Pending {
        state.pending = AddPrefix::None;
        match key.code {
            KeyCode::Char('f') => {
                state.focus = FocusTarget::Input;
                return None;
            }
            KeyCode::Char('d') => {
                create_new_folder(state);
                return None;
            }
            _ => {}
        }
    }

    // ── Delete confirmation mode ──
    if state.confirm_delete.is_some() {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                if let Some(ref path_str) = state.confirm_delete.clone() {
                    let path = std::path::Path::new(path_str);
                    if Some(path) == state.active_vault_path.as_deref() {
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

    // ── Locations overlay ──
    if state.focus == FocusTarget::Locations {
        return handle_locations_key(key, state);
    }

    // ── Browser-wide keys ──
    if is_toggle_hidden_key(&key) {
        // Never let Ctrl+h / Ctrl+Backspace become search or field text.
        if state.search_active || state.focus != FocusTarget::List {
            return None;
        }
        state.show_hidden = !state.show_hidden;
        state.refresh_entries();
        return None;
    }

    if state.focus == FocusTarget::List && !state.search_active && is_locations_key(&key) {
        open_locations(state);
        return None;
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
        KeyCode::Esc if state.focus == FocusTarget::Input => {
            // In the name input Esc steps back to the list, keeping the draft.
            state.focus = FocusTarget::List;
            return None;
        }
        KeyCode::Esc if state.focus != FocusTarget::Rename => {
            return Some(FileBrowserAction::Cancelled);
        }
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
        // Handled above; the overlay never reaches the focus dispatch.
        FocusTarget::Locations => None,
    }
}

/// `D` opens the Locations overlay from the list.
fn is_locations_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('D')
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}

/// Ctrl+h toggles hidden entries.  Crossterm reports byte `0x08` (Ctrl+H on
/// Unix) as `Char('h')` + CONTROL; Windows terminals may deliver it as
/// `Backspace` + CONTROL instead, so both are accepted.
fn is_toggle_hidden_key(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(
            key.code,
            KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Backspace
        )
}

/// Builds the Locations rows and opens the overlay.
fn open_locations(state: &mut FileBrowserState) {
    state.locations = build_locations();
    state.location_selected = preselect_location(&state.locations, &state.current_dir);
    state.location_scroll = 0;
    state.focus = FocusTarget::Locations;
}

fn handle_locations_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    let closes = matches!(key.code, KeyCode::Esc | KeyCode::Left)
        || (key.code == KeyCode::Char('h') && !key.modifiers.contains(KeyModifiers::CONTROL));
    if closes || is_locations_key(&key) {
        state.focus = FocusTarget::List;
        return None;
    }

    let len = state.locations.len();
    if len == 0 {
        return None;
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            state.location_selected = (state.location_selected + 1) % len;
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.location_selected = if state.location_selected == 0 {
                len - 1
            } else {
                state.location_selected - 1
            };
        }
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
            let Some(location) = state.locations.get(state.location_selected).cloned() else {
                state.focus = FocusTarget::List;
                return None;
            };
            match list_entries(&location.path, state.show_hidden) {
                Ok(entries) => {
                    state.current_dir = location.path;
                    state.entries = entries;
                    state.selected = 0;
                    state.search_active = false;
                    state.search_query.clear();
                    state.focus = FocusTarget::List;
                }
                Err(e) => state.error = Some(format!("{e}")),
            }
        }
        _ => {}
    }

    None
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
            KeyCode::Char('a') if state.mode == FileBrowserMode::CreateFile => {
                state.pending = AddPrefix::Pending;
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
                    let path = entry.path.clone();
                    match list_entries(&path, state.show_hidden) {
                        Ok(new_entries) => {
                            state.current_dir = path;
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
        KeyCode::Char('a') if state.mode == FileBrowserMode::CreateFile => {
            state.pending = AddPrefix::Pending;
            None
        }
        KeyCode::Char('r') => {
            if let Some(entry) = state.entries.get(state.selected)
                && entry.is_age_file
                && !entry.is_dir
                && state.mode != FileBrowserMode::ImportFile
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
                && entry.is_age_file
                && !entry.is_dir
                && state.mode != FileBrowserMode::ImportFile
            {
                state.confirm_delete = Some(entry.path.to_string_lossy().to_string());
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
            if full_path.exists() {
                state.error = Some(texts::MSG_FILE_EXISTS.to_string());
                return None;
            }
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

fn go_to_parent(state: &mut FileBrowserState) {
    let parent = match state.current_dir.parent() {
        Some(p) => p.to_path_buf(),
        None => return,
    };
    if parent == state.current_dir {
        return;
    }

    let prev_dir = state.current_dir.clone();
    match list_entries(&parent, state.show_hidden) {
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
            let dir = state.current_dir.clone();
            if let Ok(new_entries) = list_entries(&dir, state.show_hidden) {
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
    use crate::file_browser::{FileBrowserMode, FileBrowserSession, Location};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_state(mode: FileBrowserMode) -> FileBrowserState {
        let mut state = FileBrowserState::new(mode, &FileBrowserSession::default());
        // Point to /tmp so list_dir works and entries are populated
        state.current_dir = std::path::PathBuf::from("/tmp");
        state.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap_or_default();
        state.selected = 0;
        state
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    // ── `a` prefix in CreateFile mode ──

    #[test]
    fn test_a_prefix_then_f_focuses_input() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        assert_eq!(state.focus, FocusTarget::List);

        assert_eq!(handle_key(key(KeyCode::Char('a')), &mut state), None);
        assert_eq!(state.pending, AddPrefix::Pending);
        // Still on the list — the prefix waits for the follow-up key.
        assert_eq!(state.focus, FocusTarget::List);

        assert_eq!(handle_key(key(KeyCode::Char('f')), &mut state), None);
        assert_eq!(state.pending, AddPrefix::None);
        assert_eq!(state.focus, FocusTarget::Input);
    }

    #[test]
    fn test_a_prefix_then_d_creates_folder() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        let before = state.entries.len();

        assert_eq!(handle_key(key(KeyCode::Char('a')), &mut state), None);
        assert_eq!(handle_key(key(KeyCode::Char('d')), &mut state), None);

        assert_eq!(state.pending, AddPrefix::None);
        assert_eq!(state.focus, FocusTarget::List);
        let created = state.current_dir.join(texts::NEW_FOLDER_DEFAULT);
        assert!(created.is_dir(), "folder should have been created");
        assert_eq!(state.entries.len(), before + 1);

        let _ = std::fs::remove_dir(&created);
    }

    #[test]
    fn test_a_prefix_other_key_cancels() {
        let mut state = make_state(FileBrowserMode::CreateFile);

        handle_key(key(KeyCode::Char('a')), &mut state);
        assert_eq!(state.pending, AddPrefix::Pending);

        let result = handle_key(key(KeyCode::Char('x')), &mut state);
        assert_eq!(result, None);
        assert_eq!(state.pending, AddPrefix::None);
        assert_eq!(state.focus, FocusTarget::List);
    }

    #[test]
    fn test_a_prefix_switches_to_input_on_empty_list() {
        let mut state =
            FileBrowserState::new(FileBrowserMode::CreateFile, &FileBrowserSession::default());
        state.current_dir = std::path::PathBuf::from("/tmp/__empty_test_dir__");
        let _ = std::fs::create_dir_all(&state.current_dir);
        state.entries = file_handling::list_dir(&state.current_dir).unwrap();
        assert_eq!(state.filtered_len(), 0);

        handle_key(key(KeyCode::Char('a')), &mut state);
        let result = handle_key(key(KeyCode::Char('f')), &mut state);
        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::Input);

        let _ = std::fs::remove_dir(&state.current_dir);
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

    // ── Error popup dismissal ──

    #[test]
    fn test_any_key_dismisses_error_and_keeps_draft() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "taken.age".to_string();
        state.creating_cursor = state.creating_name.len();
        state.error = Some(texts::MSG_FILE_EXISTS.to_string());

        // First key dismisses the error popup and must not leak into the field.
        let result = handle_key(key(KeyCode::Char('x')), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.error, None);
        assert_eq!(state.creating_name, "taken.age", "draft must be preserved");
        assert_eq!(state.focus, FocusTarget::Input);

        // The next key edits normally.
        handle_key(key(KeyCode::Char('x')), &mut state);
        assert_eq!(state.creating_name, "taken.agex");
    }

    // ── Full create-file flow ──

    #[test]
    fn test_full_flow_create_file() {
        let dir = std::path::PathBuf::from("/tmp/__test_create_flow__");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut state =
            FileBrowserState::new(FileBrowserMode::CreateFile, &FileBrowserSession::default());
        state.current_dir = dir.clone();
        state.entries = file_handling::list_dir(&dir).unwrap_or_default();

        // Step 1: a f → focus becomes Input
        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('f')), &mut state);
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

        let _ = std::fs::remove_dir_all(&dir);
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

    // ── Hidden entries (Ctrl+h) ──

    #[test]
    fn test_ctrl_h_toggles_hidden_entries() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".hidden.txt"), b"hidden").unwrap();
        std::fs::write(dir.path().join("visible.txt"), b"visible").unwrap();

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries = file_handling::list_dir(dir.path()).unwrap();
        state.selected = state
            .entries
            .iter()
            .position(|e| e.name == "visible.txt")
            .unwrap();
        assert!(!state.show_hidden);

        assert_eq!(handle_key(ctrl(KeyCode::Char('h')), &mut state), None);

        assert!(state.show_hidden);
        assert!(state.entries.iter().any(|e| e.name == ".hidden.txt"));
        assert_eq!(
            state.entries[state.selected].name, "visible.txt",
            "selection should be preserved by name"
        );
    }

    #[test]
    fn test_ctrl_backspace_toggles_hidden_entries() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        assert!(!state.show_hidden);

        handle_key(ctrl(KeyCode::Backspace), &mut state);

        assert!(state.show_hidden);
    }

    #[test]
    fn test_ctrl_h_ignored_while_typing() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "draft".to_string();
        state.creating_cursor = state.creating_name.len();

        handle_key(ctrl(KeyCode::Char('h')), &mut state);

        assert!(!state.show_hidden);
        assert_eq!(state.creating_name, "draft");
    }

    #[test]
    fn test_ctrl_h_ignored_in_search() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.search_active = true;
        state.search_query = "abc".to_string();

        handle_key(ctrl(KeyCode::Char('h')), &mut state);

        assert!(!state.show_hidden);
        assert_eq!(state.search_query, "abc");
    }

    // ── Locations overlay (D) ──

    fn location(label: &str, path: &str) -> Location {
        Location {
            label: label.to_string(),
            path: std::path::PathBuf::from(path),
            detail: String::new(),
            is_home: false,
        }
    }

    #[test]
    fn test_d_opens_locations_overlay() {
        let mut state = make_state(FileBrowserMode::OpenFile);

        handle_key(key(KeyCode::Char('D')), &mut state);

        assert_eq!(state.focus, FocusTarget::Locations);
        assert!(state.locations.first().is_some_and(|l| l.is_home));
    }

    #[test]
    fn test_d_closes_locations_overlay() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        handle_key(key(KeyCode::Char('D')), &mut state);
        assert_eq!(state.focus, FocusTarget::Locations);

        handle_key(key(KeyCode::Char('D')), &mut state);

        assert_eq!(state.focus, FocusTarget::List);
    }

    #[test]
    fn test_esc_closes_locations_overlay_without_cancelling() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        handle_key(key(KeyCode::Char('D')), &mut state);

        let result = handle_key(key(KeyCode::Esc), &mut state);

        assert_eq!(result, None, "Esc closes the overlay, not the browser");
        assert_eq!(state.focus, FocusTarget::List);
    }

    #[test]
    fn test_locations_navigation_wraps() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.locations = vec![location("a", "/a"), location("b", "/b")];
        state.location_selected = 0;
        state.focus = FocusTarget::Locations;

        handle_key(key(KeyCode::Char('k')), &mut state);
        assert_eq!(state.location_selected, 1);

        handle_key(key(KeyCode::Char('j')), &mut state);
        assert_eq!(state.location_selected, 0);
    }

    #[test]
    fn test_locations_enter_jumps_to_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("inside.txt"), b"x").unwrap();

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.locations = vec![Location {
            label: dir.path().to_string_lossy().to_string(),
            path: dir.path().to_path_buf(),
            detail: String::new(),
            is_home: false,
        }];
        state.location_selected = 0;
        state.focus = FocusTarget::Locations;
        state.search_query = "stale".to_string();
        state.search_active = true;

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::List);
        assert_eq!(state.current_dir, dir.path());
        assert!(state.entries.iter().any(|e| e.name == "inside.txt"));
        assert!(state.search_query.is_empty());
        assert!(!state.search_active);
    }

    #[test]
    fn test_locations_enter_unreadable_keeps_overlay_open() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.locations = vec![Location {
            label: "missing".to_string(),
            path: missing.clone(),
            detail: String::new(),
            is_home: false,
        }];
        state.location_selected = 0;
        state.focus = FocusTarget::Locations;

        handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(state.focus, FocusTarget::Locations);
        assert!(state.error.is_some());
        assert_ne!(state.current_dir, missing);
    }

    #[test]
    fn test_locations_respects_hidden_setting() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".hidden.txt"), b"hidden").unwrap();

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.show_hidden = true;
        state.locations = vec![Location {
            label: dir.path().to_string_lossy().to_string(),
            path: dir.path().to_path_buf(),
            detail: String::new(),
            is_home: false,
        }];
        state.location_selected = 0;
        state.focus = FocusTarget::Locations;

        handle_key(key(KeyCode::Enter), &mut state);

        assert!(state.entries.iter().any(|e| e.name == ".hidden.txt"));
    }
}
