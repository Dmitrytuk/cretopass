//! Shared fixtures for the render submodules' tests.

use ratatui::Terminal;
use ratatui::backend::TestBackend;

use crate::file_browser::render_file_browser;
use crate::file_browser::state::{FileBrowserMode, FileBrowserSession, FileBrowserState};
use crate::file_handling;

/// A terminal for testing.
pub(super) fn make_terminal() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(80, 24)).unwrap()
}

pub(super) fn new_state(mode: FileBrowserMode) -> FileBrowserState {
    let mut state = FileBrowserState::new(mode, &FileBrowserSession::default());
    // Settle the constructor's listing so no request is left in flight and the
    // renderer sees a stable directory.
    state.settle();
    state
}

/// Helper to populate state with entries from /tmp (known to exist).
pub(super) fn populate_tmp(state: &mut FileBrowserState) {
    state.current_dir = std::path::PathBuf::from("/tmp");
    state.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap();
    state.selected = 0;
}

/// Renders and returns the full buffer content as a single string.
pub(super) fn render_to_text(state: &mut FileBrowserState) -> String {
    let mut terminal = make_terminal();
    terminal
        .draw(|f| {
            let area = f.area();
            render_file_browser(f, area, state);
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    buffer
        .content
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
}

/// A share browser over `dir`, as if opened for the secret file `dir/vault.age`.
pub(super) fn share_state(dir: &std::path::Path) -> FileBrowserState {
    let mut state = FileBrowserState::for_share(
        Some(&dir.join("vault.age")),
        None,
        &FileBrowserSession::default(),
    );
    state.settle();
    state
}
