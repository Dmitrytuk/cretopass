//! Clipboard-paste routing tests.

use super::*;

// ── Clipboard paste routing ──
#[test]
fn ctrl_v_classifies_as_paste_even_with_a_modal_open() {
    // With a modal open, `Ctrl+V` still pastes rather than reaching it.
    let with_modal = editing_state();
    assert_eq!(
        route_key(&ctrl(KeyCode::Char('v')), with_modal.modal.is_some()),
        KeyRoute::Paste
    );
    // A bare `v` reaches the modal, where it types.
    assert_eq!(
        route_key(&key(KeyCode::Char('v')), with_modal.modal.is_some()),
        KeyRoute::Modal
    );
    // With no modal, `Ctrl+V` pastes instead of navigating.
    let no_modal = TreeState::new(Node::new("Root"));
    assert_eq!(
        route_key(&ctrl(KeyCode::Char('v')), no_modal.modal.is_some()),
        KeyRoute::Paste
    );
}

#[test]
fn a_failed_clipboard_read_leaves_the_form_untouched() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut state);
    let before = form_of(&state).scratch.as_ref().map(|s| s.text.clone());

    apply_paste_result(
        Err(clipboard::ClipboardError::Read("no display".to_string())),
        &mut state,
    );

    assert_eq!(
        form_of(&state).scratch.as_ref().map(|s| s.text.clone()),
        before,
        "an unreadable clipboard must change nothing"
    );
}

#[test]
fn a_clipboard_read_reaches_the_open_form() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut state);

    apply_paste_result(Ok(Zeroizing::new("pw".to_string())), &mut state);

    assert_eq!(form_of(&state).scratch.as_ref().unwrap().text, "Rootpw");
}

#[test]
fn an_empty_clipboard_read_changes_nothing() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut state);

    apply_paste_result(Ok(Zeroizing::new(String::new())), &mut state);

    assert_eq!(form_of(&state).scratch.as_ref().unwrap().text, "Root");
}

#[test]
fn a_paste_with_no_surface_skips_the_clipboard_read() {
    let state = TreeState::new(Node::new("Root"));
    assert!(matches!(state.mode, InputMode::Normal));
    let mut paste = None;

    start_paste(&state, &mut paste);

    assert!(
        paste.is_none(),
        "no clipboard worker is started with no paste surface"
    );
}

#[test]
fn the_term_native_and_ctrl_v_sources_reach_the_same_field() {
    let mut via_chord = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut via_chord);
    apply_paste_result(Ok(Zeroizing::new("shared".to_string())), &mut via_chord);

    // `Event::Paste` calls `apply_paste` directly; the chord reaches it
    // through `apply_paste_result`. Both must land in the same place.
    let mut via_term = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut via_term);
    apply_paste("shared", &mut via_term);

    let scratch = |state: &TreeState| form_of(state).scratch.as_ref().map(|s| s.text.clone());
    assert_eq!(scratch(&via_chord), scratch(&via_term));
    assert_eq!(scratch(&via_term).as_deref(), Some("Rootshared"));
}

#[test]
fn a_paste_fills_the_tree_search_box_single_line() {
    let mut state = TreeState::new(Node::new("Root"));
    state.mode = InputMode::Search;

    apply_paste("Ru\r\nst", &mut state);

    assert_eq!(state.search_query, "Rust");
}
