use crossterm::event::{KeyCode, KeyEvent};

use super::fs;
use super::state::{FileBrowserAction, FileBrowserMode, FileBrowserState, FileFilter, FocusTarget};

/// Processes a key event and returns an action if the user completed
/// or cancelled the file browser.  Returns `None` when the browser
/// state was just mutated (re-render needed).
pub fn handle_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
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
            KeyCode::Tab if state.mode == FileBrowserMode::CreateFile => {
                state.focus = FocusTarget::Input;
                None
            }
            _ => None,
        };
    }

    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let pos = indices.iter().position(|&i| i == state.selected).unwrap_or(0);
            let next_pos = (pos + 1) % indices.len();
            state.selected = indices[next_pos];
            None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let pos = indices.iter().position(|&i| i == state.selected).unwrap_or(0);
            let prev_pos = if pos == 0 { indices.len() - 1 } else { pos - 1 };
            state.selected = indices[prev_pos];
            None
        }
        KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
            let idx = state.selected;
            if let Some(entry) = state.entries.get(idx) {
                if entry.is_dir {
                    match fs::list_dir(&entry.path) {
                        Ok(new_entries) => {
                            state.current_dir = entry.path.clone();
                            state.entries = new_entries;
                            state.selected = 0;
                            state.scroll = 0;
                            state.search_active = false;
                            state.search_query.clear();
                        }
                        Err(e) => state.error = Some(e),
                    }
                } else if state.mode == FileBrowserMode::OpenFile {
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
        KeyCode::Char('n') => {
            create_new_folder(state);
            None
        }
        KeyCode::Tab if state.mode == FileBrowserMode::CreateFile => {
            state.focus = FocusTarget::Input;
            None
        }
        _ => None,
    }
}

fn handle_input_key(key: KeyEvent, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    match key.code {
        KeyCode::Char(c) => {
            state.creating_name.insert(state.creating_cursor, c);
            state.creating_cursor += 1;
            None
        }
        KeyCode::Backspace => {
            if state.creating_cursor > 0 {
                state.creating_cursor -= 1;
                state.creating_name.remove(state.creating_cursor);
            }
            None
        }
        KeyCode::Left => {
            if state.creating_cursor > 0 {
                state.creating_cursor -= 1;
            }
            None
        }
        KeyCode::Right => {
            if state.creating_cursor < state.creating_name.len() {
                state.creating_cursor += 1;
            }
            None
        }
        KeyCode::Home => {
            state.creating_cursor = 0;
            None
        }
        KeyCode::End => {
            state.creating_cursor = state.creating_name.len();
            None
        }
        KeyCode::Tab | KeyCode::Esc => {
            state.focus = FocusTarget::List;
            None
        }
        KeyCode::Enter => {
            let name = state.creating_name.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let name = if name.contains('.') {
                name
            } else {
                format!("{}.age", name)
            };
            let full_path = state.current_dir.join(&name);
            Some(FileBrowserAction::Selected(
                full_path.to_string_lossy().to_string(),
            ))
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
    match fs::list_dir(&parent) {
        Ok(new_entries) => {
            let prev_name = prev_dir.file_name().map(|n| n.to_string_lossy().to_string());

            state.current_dir = parent;
            state.entries = new_entries;
            state.search_active = false;
            state.search_query.clear();

            state.selected = prev_name
                .and_then(|name| state.entries.iter().position(|e| e.name == name))
                .unwrap_or(0);
            state.clamp_selection();
        }
        Err(e) => state.error = Some(e),
    }
}

fn create_new_folder(state: &mut FileBrowserState) {
    let mut name = "New Folder".to_string();
    let mut counter = 1;
    while state.current_dir.join(&name).exists() {
        counter += 1;
        name = format!("New Folder ({})", counter);
    }

    let new_path = state.current_dir.join(&name);
    match fs::create_dir(&new_path) {
        Ok(()) => {
            if let Ok(new_entries) = fs::list_dir(&state.current_dir) {
                state.entries = new_entries;
                // Select the newly created folder
                state.selected = state
                    .entries
                    .iter()
                    .position(|e| e.name == name)
                    .unwrap_or(0);
                state.clamp_selection();
            }
        }
        Err(e) => state.error = Some(e),
    }
}
