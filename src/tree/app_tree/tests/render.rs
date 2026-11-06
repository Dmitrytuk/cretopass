//! Frame-painting and title-bar rendering tests.

use super::*;

#[test]
fn overlays_render_on_a_terminal_smaller_than_the_overlay() {
    // A short terminal used to underflow `area.height / 2 - n` and panic
    // while drawing an overlay; every overlay now centres with saturating
    // arithmetic.
    let mut terminal = ratatui::Terminal::new(TestBackend::new(4, 2)).unwrap();

    let mut alert = TreeState::new(Node::new("Root"));
    alert.modal = Some(Modal::error("boom"));
    terminal.draw(|f| ui(f, &mut alert)).unwrap();

    let mut search = TreeState::new(Node::new("Root"));
    search.mode = InputMode::Search;
    terminal.draw(|f| ui(f, &mut search)).unwrap();

    let mut discard = TreeState::new(Node::new("Root"));
    discard.save_exit = SaveExit::ConfirmDiscard;
    terminal.draw(|f| ui(f, &mut discard)).unwrap();
}

/// Renders `ui` into a test backend and asserts that no cell falls through
/// to the terminal's own background (see
/// [ADR 0018](../../../../docs/adr/0018-app-owned-background-fill.md)).
fn assert_no_terminal_background(state: &mut TreeState) {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui(frame, state)).unwrap();

    let buffer = terminal.backend().buffer();
    for (i, cell) in buffer.content.iter().enumerate() {
        assert_ne!(
            cell.style().bg,
            Some(Color::Reset),
            "cell {i} ({:?}) fell through to the terminal background",
            cell.symbol()
        );
    }
}

#[test]
fn tree_screen_paints_every_cell() {
    let mut state = TreeState::new(crate::build_demo_tree());
    assert_no_terminal_background(&mut state);
}

#[test]
fn search_overlay_paints_every_cell() {
    // The Search overlay clears its rect before drawing the block, the path
    // most likely to expose the terminal background.
    let mut state = TreeState::new(Node::new("Root"));
    state.mode = InputMode::Search;
    state.search_query = "Ro".to_string();
    assert_no_terminal_background(&mut state);
}

/// The Tree title bar shows a blocking operation's status ahead of the
/// autosave status, with its busy glyph.
#[test]
fn a_blocking_operation_shows_the_saving_spinner_in_the_title_bar() {
    let mut state = TreeState::new(Node::new("Root"));
    state.save_state = SaveState::Idle;
    state.operation_status = Some(crate::theming::Status::Saving);

    // Read the frame the state itself carries, not a freshly detected one, so a
    // regression that changes the default frame set cannot slip through.
    let glyph = state.spinner.glyph();
    let content = rendered_text(&mut state);
    assert!(
        content.contains(texts::STATUS_SAVING),
        "the title bar must show the operation status: {content:?}"
    );
    assert!(
        content.contains(glyph),
        "the busy glyph must be shown: {content:?}"
    );
}
