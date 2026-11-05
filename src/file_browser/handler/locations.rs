//! The `D` Locations overlay: its key map and the row builder call.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::file_browser::state::{
    FileBrowserAction, FileBrowserState, FocusTarget, build_locations, preselect_location,
};

/// `D` opens the Locations overlay from the list.
pub(super) fn is_locations_key(key: &KeyEvent) -> bool {
    key.code == KeyCode::Char('D')
        && !key.modifiers.contains(KeyModifiers::CONTROL)
        && !key.modifiers.contains(KeyModifiers::ALT)
}
/// Builds the Locations rows and opens the overlay.
pub(super) fn open_locations(state: &mut FileBrowserState) {
    state.locations = build_locations();
    state.location_selected = preselect_location(&state.locations, &state.current_dir);
    state.location_scroll = 0;
    state.focus = FocusTarget::Locations;
}
pub(super) fn handle_locations_key(
    key: KeyEvent,
    state: &mut FileBrowserState,
) -> Option<FileBrowserAction> {
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
            // Close the overlay and request the directory in the background; a
            // failure surfaces once the listing lands, restoring the old one.
            state.focus = FocusTarget::List;
            state.navigate_to(location.path, None);
        }
        _ => {}
    }

    None
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::super::fixtures::{key, make_state};
    use crate::file_browser::FileBrowserMode;
    use crate::file_browser::Location;
    use crate::file_browser::handle_key;
    use crate::file_browser::state::FocusTarget;

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
        assert_eq!(state.focus, FocusTarget::List, "the overlay closes at once");
        assert_eq!(state.current_dir, dir.path(), "the jump is immediate");
        state.settle();
        assert!(state.entries.iter().any(|e| e.name == "inside.txt"));
        assert!(state.search_query.is_empty());
        assert!(!state.search_active);
    }

    #[test]
    fn test_locations_enter_unreadable_restores_the_previous_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing");

        let mut state = make_state(FileBrowserMode::OpenFile);
        let before = state.current_dir.clone();
        state.locations = vec![Location {
            label: "missing".to_string(),
            path: missing.clone(),
            detail: String::new(),
            is_home: false,
        }];
        state.location_selected = 0;
        state.focus = FocusTarget::Locations;

        handle_key(key(KeyCode::Enter), &mut state);
        state.settle();

        assert_eq!(state.focus, FocusTarget::List, "the overlay closes at once");
        assert!(state.error.is_some(), "the failure is reported");
        assert_eq!(
            state.current_dir, before,
            "the previous directory is restored"
        );
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
        state.settle();

        assert!(state.entries.iter().any(|e| e.name == ".hidden.txt"));
    }
}
