//! The File Browser's top-level key dispatch.
//!
//! [`handle_key`] peels off the transient routing states — the error popup, the
//! `a` add-file/dir prefix, the confirmations, the Locations overlay and the
//! search box — before dispatching to the handler for the focused area.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton};

use crate::cursor::PastePolicy;
use crate::file_browser::state::{
    AddPrefix, BrowserHit, FileBrowserAction, FileBrowserState, FocusTarget,
};
use crate::file_handling;
use crate::hitbox::is_left;
use crate::theming::texts;

use super::input::{handle_input_key, handle_input_paste};
use super::list::handle_list_key;
use super::locations::{handle_locations_key, is_locations_key, open_locations};
use super::rename::{handle_rename_key, handle_rename_paste};

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
                state.creating_is_dir = false;
                state.focus = FocusTarget::Input;
                return None;
            }
            KeyCode::Char('d') => {
                state.creating_is_dir = true;
                state.focus = FocusTarget::Input;
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
                        // The refreshed listing tells whether the name really
                        // went away, or a sync client still holds it.
                        state.note_deleted_path(path.to_path_buf());
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

    // ── Overwrite confirmation (export file or share folder) ──
    if state.confirm_overwrite.is_some() {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Enter => state
                .confirm_overwrite
                .take()
                .map(FileBrowserAction::Selected),
            _ => {
                state.confirm_overwrite = None;
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
            KeyCode::Char(c) if !c.is_control() => {
                state.search_query.push(c);
                state.clamp_selection();
                return None;
            }
            // A control character is consumed and dropped, never routed on.
            KeyCode::Char(_) => return None,
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
            state.creating_is_dir = false;
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

/// Applies pasted `text` to the browser's active text surface.
///
/// Mirrors [`handle_key`]'s routing: the error popup dismisses on the paste
/// rather than swallowing it into the field, the transient confirmations and
/// the Locations overlay own the keyboard and ignore it, and only the two text
/// inputs and the search box accept text. The list itself has nothing to paste
/// into.
pub fn handle_paste(text: &str, state: &mut FileBrowserState) {
    let cleaned = PastePolicy::SingleLine.clean(text);
    if cleaned.is_empty() {
        return;
    }

    if state.error.is_some() {
        state.error = None;
        return;
    }

    if state.pending == AddPrefix::Pending
        || state.confirm_delete.is_some()
        || state.confirm_overwrite.is_some()
        || state.focus == FocusTarget::Locations
    {
        return;
    }

    match state.focus {
        FocusTarget::Input => handle_input_paste(state, &cleaned),
        FocusTarget::Rename => handle_rename_paste(state, &cleaned),
        FocusTarget::List if state.search_active => {
            state.search_query.push_str(&cleaned);
            state.clamp_selection();
        }
        FocusTarget::List | FocusTarget::Locations => {}
    }
}

/// Applies a click at `(x, y)` in the File Browser.
///
/// Mirrors [`handle_key`]'s routing: a transient popup owns the screen and takes
/// no clicks, and only the file list or the Locations overlay has clickable rows.
/// A click on a directory row opens it (like `Enter`); a click on a file row
/// selects it, and a repeat click on the already-selected file activates it — so
/// a stray click never opens the wrong file.
pub fn handle_click(
    button: MouseButton,
    x: u16,
    y: u16,
    state: &mut FileBrowserState,
) -> Option<FileBrowserAction> {
    if !is_left(button) {
        return None;
    }
    if state.error.is_some()
        || state.pending == AddPrefix::Pending
        || state.confirm_delete.is_some()
        || state.confirm_overwrite.is_some()
    {
        return None;
    }
    match state.hits.hit(x, y, button) {
        Some(BrowserHit::Entry(index)) => click_entry(index, state),
        Some(BrowserHit::Location(index)) => click_location(index, state),
        None => None,
    }
}

/// Applies a click on a file-list row.
fn click_entry(index: usize, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    if index >= state.entries.len() {
        return None;
    }
    let was_selected = state.selected == index;
    state.selected = index;
    // The search box takes `Enter` to end the search; a click must act, so it
    // leaves the search box first.
    state.search_active = false;
    let is_dir = state.entries[index].is_dir;
    if is_dir || was_selected {
        handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), state)
    } else {
        None
    }
}

/// Applies a click on a Locations overlay row: select it and open it.
fn click_location(index: usize, state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    if index >= state.locations.len() {
        return None;
    }
    state.location_selected = index;
    handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), state)
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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, MouseButton};

    use super::super::fixtures::{ctrl, key, make_state};
    use crate::file_browser::state::{AddPrefix, BrowserHit, FocusTarget};
    use crate::file_browser::{
        FileBrowserAction, FileBrowserMode, FileBrowserSession, FileBrowserState,
    };
    use crate::file_browser::{handle_click, handle_key, handle_paste};
    use crate::file_handling;
    use crate::theming::texts;

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
    fn test_a_prefix_then_d_focuses_input_for_new_folder() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        let before = state.entries.len();

        assert_eq!(handle_key(key(KeyCode::Char('a')), &mut state), None);
        assert_eq!(handle_key(key(KeyCode::Char('d')), &mut state), None);

        assert_eq!(state.pending, AddPrefix::None);
        assert_eq!(state.focus, FocusTarget::Input);
        assert!(state.creating_is_dir, "`ad` targets a directory");

        // Type the directory name and confirm.
        for c in "myfolder".chars() {
            handle_key(key(KeyCode::Char(c)), &mut state);
        }
        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None, "creating a folder stays in the browser");
        assert_eq!(state.focus, FocusTarget::List);
        assert!(!state.creating_is_dir);
        assert!(state.creating_name.is_empty());
        let created = state.current_dir.join("myfolder");
        assert!(created.is_dir(), "folder should have been created");
        state.settle();
        assert_eq!(state.entries.len(), before + 1);

        let _ = std::fs::remove_dir(&created);
    }

    #[test]
    fn a_control_char_is_rejected_by_the_name_input() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('d')), &mut state); // `ad` focuses the input
        assert_eq!(state.focus, FocusTarget::Input);

        handle_key(key(KeyCode::Char('x')), &mut state);
        handle_key(key(KeyCode::Char('\u{9b}')), &mut state); // C1 CSI
        handle_key(key(KeyCode::Char('\u{1b}')), &mut state); // ESC

        assert_eq!(state.creating_name, "x", "control chars are dropped");
    }

    #[test]
    fn a_control_char_is_rejected_by_the_search_box() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.search_active = true;

        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('\u{9b}')), &mut state);

        assert_eq!(state.search_query, "a", "control chars are dropped");
    }

    #[test]
    fn test_ad_existing_name_shows_error_and_stays_in_input() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        let existing = state.current_dir.join("taken_dir");
        let _ = std::fs::remove_dir_all(&existing);
        std::fs::create_dir_all(&existing).unwrap();

        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('d')), &mut state);
        for c in "taken_dir".chars() {
            handle_key(key(KeyCode::Char(c)), &mut state);
        }
        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.error.as_deref(), Some(texts::MSG_FILE_EXISTS));
        assert_eq!(
            state.focus,
            FocusTarget::Input,
            "stays in the input on error"
        );

        let _ = std::fs::remove_dir(&existing);
    }

    #[test]
    fn test_esc_in_input_clears_dir_flag() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('d')), &mut state);
        assert!(state.creating_is_dir);

        handle_key(key(KeyCode::Esc), &mut state);

        assert_eq!(state.focus, FocusTarget::List);
        assert!(!state.creating_is_dir);
    }

    #[test]
    fn test_af_after_ad_creates_file_not_dir() {
        let mut state = make_state(FileBrowserMode::CreateFile);

        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('d')), &mut state);
        handle_key(key(KeyCode::Esc), &mut state);

        handle_key(key(KeyCode::Char('a')), &mut state);
        handle_key(key(KeyCode::Char('f')), &mut state);

        assert!(!state.creating_is_dir, "`af` must clear the directory flag");
        assert_eq!(state.focus, FocusTarget::Input);
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
    // ── Hidden entries (Ctrl+h) ──

    /// A listed file entry for `path`, for stubbed listing results.
    fn entry(path: &std::path::Path) -> crate::file_handling::FileEntry {
        crate::file_handling::FileEntry {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            path: path.to_path_buf(),
            is_dir: false,
            looks_like_vault: true,
        }
    }

    #[test]
    fn a_normal_delete_reports_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("gone.age");
        std::fs::write(&file, b"x").unwrap();

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries = vec![entry(&file)];
        state.selected = 0;

        handle_key(key(KeyCode::Char('d')), &mut state);
        handle_key(key(KeyCode::Char('y')), &mut state);
        state.settle();

        assert!(
            state.error.is_none(),
            "a completed delete must be silent: {:?}",
            state.error
        );
        assert!(!state.entries.iter().any(|e| e.name == "gone.age"));
    }

    #[test]
    fn a_deleted_file_still_listed_reports_that_the_delete_is_pending() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("held.age");
        std::fs::write(&file, b"x").unwrap();

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries = vec![entry(&file)];
        state.selected = 0;

        handle_key(key(KeyCode::Char('d')), &mut state);
        handle_key(key(KeyCode::Char('y')), &mut state);
        assert!(!file.exists(), "the delete itself succeeded");

        // Stubbed listing result: a sync client still holds the name, so the
        // refreshed directory still lists it.
        state.entries = vec![entry(&file)];
        state.reconcile_pending_delete();

        assert_eq!(
            state.error.as_deref(),
            Some(texts::MSG_DELETE_PENDING),
            "a delete that has not taken effect must be explained"
        );
    }

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
        state.settle();

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

    // ── Clipboard paste ──

    #[test]
    fn paste_into_the_name_input_joins_lines() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name.clear();
        state.creating_cursor = 0;

        handle_paste("a\r\nb", &mut state);

        assert_eq!(state.creating_name, "ab");
        assert_eq!(state.creating_cursor, 2);
    }

    #[test]
    fn paste_into_the_rename_input_joins_lines() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.focus = FocusTarget::Rename;
        state.rename_name.clear();
        state.rename_cursor = 0;

        handle_paste("x\r\ny", &mut state);

        assert_eq!(state.rename_name, "xy");
        assert_eq!(state.rename_cursor, 2);
    }

    #[test]
    fn paste_into_the_search_box_appends() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.focus = FocusTarget::List;
        state.search_active = true;
        state.search_query.clear();

        handle_paste("ab", &mut state);

        assert_eq!(state.search_query, "ab");
    }

    #[test]
    fn paste_is_ignored_while_the_locations_overlay_is_open() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.focus = FocusTarget::Locations;
        // A live search box, which the paste would otherwise fill: the overlay
        // must own the keyboard and swallow it. (The guard and the `Locations`
        // match arm both ignore a paste here; the test pins the observable
        // outcome, not which of the two did it.)
        state.search_active = true;
        state.search_query = "keep".to_string();

        handle_paste("x", &mut state);

        assert_eq!(state.search_query, "keep");
    }

    #[test]
    fn a_paste_dismisses_an_error_without_leaking_into_the_field() {
        let mut state = make_state(FileBrowserMode::CreateFile);
        state.focus = FocusTarget::Input;
        state.creating_name = "taken.age".to_string();
        state.creating_cursor = state.creating_name.len();
        state.error = Some(texts::MSG_FILE_EXISTS.to_string());

        handle_paste("x", &mut state);

        assert_eq!(state.error, None);
        assert_eq!(state.creating_name, "taken.age", "the draft is preserved");
    }

    #[test]
    fn a_paste_on_the_list_without_a_search_changes_nothing() {
        let mut state = make_state(FileBrowserMode::OpenFile);
        state.focus = FocusTarget::List;
        assert!(!state.search_active);

        handle_paste("x", &mut state);

        assert!(state.search_query.is_empty());
    }

    // ── Click hit-testing ──

    /// Draws the browser so its renderer fills `state.hits`.
    fn draw(state: &mut FileBrowserState) {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| crate::file_browser::render_file_browser(frame, frame.area(), state))
            .unwrap();
    }

    /// The first cell whose click resolves to `target`.
    fn point_for(state: &FileBrowserState, target: BrowserHit) -> (u16, u16) {
        for y in 0..24u16 {
            for x in 0..80u16 {
                if state.hits.hit(x, y, MouseButton::Left) == Some(target) {
                    return (x, y);
                }
            }
        }
        panic!("no hit region for {target:?}");
    }

    #[test]
    fn a_click_on_a_directory_row_opens_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries = file_handling::list_dir(dir.path()).unwrap();
        state.focus = FocusTarget::List;

        draw(&mut state);
        let index = state.entries.iter().position(|e| e.name == "sub").unwrap();
        let (x, y) = point_for(&state, BrowserHit::Entry(index));

        handle_click(MouseButton::Left, x, y, &mut state);

        assert_eq!(
            state.current_dir,
            dir.path().join("sub"),
            "a single click opens a directory"
        );
    }

    #[test]
    fn a_repeat_click_on_a_file_activates_it() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("note.txt"), b"x").unwrap();
        std::fs::write(dir.path().join("other.txt"), b"y").unwrap();

        let mut state = make_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries = file_handling::list_dir(dir.path()).unwrap();
        state.focus = FocusTarget::List;

        draw(&mut state);
        let index = state
            .entries
            .iter()
            .position(|e| e.name == "note.txt")
            .unwrap();
        let other = state
            .entries
            .iter()
            .position(|e| e.name == "other.txt")
            .unwrap();
        // Start on a different row, so the first click is a real selection.
        state.selected = other;
        let (x, y) = point_for(&state, BrowserHit::Entry(index));

        assert_eq!(
            handle_click(MouseButton::Left, x, y, &mut state),
            None,
            "the first click only selects a file"
        );
        assert_eq!(state.selected, index);
        assert!(matches!(
            handle_click(MouseButton::Left, x, y, &mut state),
            Some(FileBrowserAction::Selected(_))
        ));
    }
}
