//! Status-bar hint and prefix-mode tests.

use super::*;

#[test]
fn normal_hint_switches_to_the_root_set() {
    let mut state = state_with_a_child();
    state.selected = 1;
    assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE);

    state.selected = 0;
    assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE_ROOT);
}

#[test]
fn normal_hint_advertises_paste_only_with_a_cut_buffer() {
    let mut state = state_with_a_child();
    state.selected = 1;
    state.cut_buffer = Some(Node::new("X"));
    assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE_PASTE);

    state.selected = 0;
    assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE_ROOT_PASTE);
}

#[test]
fn add_hint_narrows_to_a_child_on_the_root() {
    let mut state = state_with_a_child();
    state.mode = InputMode::PendingAdd;
    state.selected = 1;
    assert_eq!(status_bar_parts(&state).2, texts::HINT_PENDING_ADD);

    state.selected = 0;
    assert_eq!(status_bar_parts(&state).2, texts::HINT_PENDING_ADD_ROOT);
}

#[test]
fn paste_hint_narrows_to_a_child_on_the_root() {
    let mut state = state_with_a_child();
    state.mode = InputMode::PendingPaste;
    state.selected = 1;
    assert_eq!(status_bar_parts(&state).2, texts::HINT_PASTE);

    state.selected = 0;
    assert_eq!(status_bar_parts(&state).2, texts::HINT_PASTE_ROOT);
}

#[test]
fn cut_prefix_is_inert_on_the_root() {
    let mut state = state_with_a_child();
    state.selected = 0;
    handle_normal(key(KeyCode::Char('d')), &mut state);
    assert!(matches!(state.mode, InputMode::Normal));
}

#[test]
fn cut_prefix_is_armed_off_the_root() {
    let mut state = state_with_a_child();
    state.selected = 1;
    handle_normal(key(KeyCode::Char('d')), &mut state);
    assert!(matches!(state.mode, InputMode::PendingCut));
}

#[test]
fn add_sibling_on_the_root_cancels_the_prefix_and_opens_no_form() {
    for sibling in [KeyCode::Char('a'), KeyCode::Char('A')] {
        let mut state = state_with_a_child();
        state.selected = 0;
        handle_normal(key(KeyCode::Char('a')), &mut state);
        assert!(matches!(state.mode, InputMode::PendingAdd));

        handle_pending_add(key(sibling), &mut state);
        assert!(matches!(state.mode, InputMode::Normal));
        assert!(state.modal.is_none());
    }
}

#[test]
fn add_sibling_off_the_root_still_opens_the_form() {
    let mut state = state_with_a_child();
    state.selected = 1;
    handle_normal(key(KeyCode::Char('a')), &mut state);
    handle_pending_add(key(KeyCode::Char('a')), &mut state);
    assert_eq!(state.add_position, AddPosition::SiblingAfter);
    assert!(detail(&state).form().is_some());
}

#[test]
fn add_child_on_the_root_creates_a_child_node() {
    let mut state = state_with_a_child();
    state.selected = 0;
    handle_normal(key(KeyCode::Char('a')), &mut state);
    handle_pending_add(key(KeyCode::Char('l')), &mut state);
    assert_eq!(state.add_position, AddPosition::Child);
    assert!(detail(&state).form().is_some());

    dispatch_key(key(KeyCode::Char('i')), &mut state);
    type_text(&mut state, "New");
    dispatch_key(key(KeyCode::Enter), &mut state);

    assert!(state.root.children.iter().any(|child| child.name == "New"));
    assert!(state.modified);
}

#[test]
fn tree_hints_stay_within_the_shipped_width() {
    // The status bar is one unwrapped line and the mode badge is drawn in
    // front of the hint, so the hints must not grow past the widest one the
    // Tree already ships (`HINT_TREE_PASTE`). This is a ceiling, not a
    // terminal-width guarantee.
    const BUDGET: usize = 82;
    let hints = [
        texts::HINT_TREE,
        texts::HINT_TREE_PASTE,
        texts::HINT_TREE_ROOT,
        texts::HINT_TREE_ROOT_PASTE,
        texts::HINT_SEARCH,
        texts::HINT_PENDING_CUT,
        texts::HINT_PASTE,
        texts::HINT_PASTE_ROOT,
        texts::HINT_PENDING_ADD,
        texts::HINT_PENDING_ADD_ROOT,
        texts::HINT_DETAIL,
        texts::HINT_DETAIL_ALERT,
        texts::HINT_FORM,
        texts::HINT_FORM_EXTRA,
        texts::HINT_FORM_EXTRA_PASTE,
        texts::HINT_FORM_ADD,
        texts::HINT_FORM_INPUT,
        texts::HINT_FORM_GENERATOR_INPUT,
        texts::HINT_GENERATOR,
        texts::HINT_SYMBOLS,
        texts::HINT_SELECT_FIELD_TYPE,
        texts::HINT_CUSTOM_FIELD_NAME,
        texts::HINT_CONFIRM_DELETE,
        texts::HINT_FIELD_CUT,
        texts::HINT_FIELD_PASTE,
        texts::HINT_SAVE_EXIT_BLOCKED,
        texts::HINT_DISCARD_CONFIRM,
        texts::HINT_HELP,
        texts::HINT_EXPORT_NODE_SELECT,
        texts::HINT_EXPORT_NODE_FORMAT,
        texts::HINT_PENDING_EXPORT,
        texts::HINT_SHARE_SCOPE,
        texts::HINT_SHARE_PASSPHRASE,
        texts::HINT_SHARE_NO_PASSPHRASE,
    ];
    for hint in hints {
        assert!(
            hint.chars().count() <= BUDGET,
            "hint exceeds the {BUDGET}-char budget: {hint:?}"
        );
    }
}
