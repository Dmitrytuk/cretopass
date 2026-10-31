//! Shared fixtures for the handler submodules' tests.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

use crate::file_browser::state::{FileBrowserMode, FileBrowserSession, FileBrowserState};
use crate::file_handling;

/// A plain key press with no modifiers.
pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// A Ctrl-modified key press.
pub(super) fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    }
}

/// A browser on `/tmp` (known to exist), with its entries listed.
pub(super) fn make_state(mode: FileBrowserMode) -> FileBrowserState {
    let mut state = FileBrowserState::new(mode, &FileBrowserSession::default());
    state.current_dir = std::path::PathBuf::from("/tmp");
    state.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap_or_default();
    state.selected = 0;
    state
}
