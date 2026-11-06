//! Form, picker and field-editing tests for the Tree's modal router.

use super::*;

fn handle_select_field_type(key: KeyEvent, state: &mut TreeState) {
    dispatch_key(key, state);
}

fn handle_confirm_delete(key: KeyEvent, state: &mut TreeState) {
    dispatch_key(key, state);
}

#[test]
fn form_opens_in_nav_and_i_enters_input() {
    let mut state = editing_state();
    assert_eq!(form_of(&state).focus, FormFocus::Nav);
    handle_form(key(KeyCode::Char('i')), &mut state);
    assert_eq!(form_of(&state).focus, FormFocus::Input);
}

#[test]
fn form_nav_enter_also_enters_input() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Enter), &mut state);
    assert_eq!(form_of(&state).focus, FormFocus::Input);
}

#[test]
fn form_nav_j_moves_row_to_extra_or_add_button() {
    let mut state = editing_state_with_fields();
    handle_form(key(KeyCode::Char('j')), &mut state);
    let form = form_of(&state);
    assert_eq!(form.selected_row, 1); // moved to Extra(0)
    assert_eq!(form.current_row(&state.root), FormRow::Extra(0));
}

#[test]
fn form_nav_esc_closes_to_detail() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Esc), &mut state);
    assert!(matches!(state.modal, Some(Modal::Detail(_))));
}

#[test]
fn empty_add_form_esc_cancels_to_normal() {
    let mut state = TreeState::new(Node::new("Root"));
    state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
    handle_form(key(KeyCode::Esc), &mut state);
    assert!(state.modal.is_none());
    assert!(matches!(state.mode, InputMode::Normal));
}

#[test]
fn editing_through_the_modal_router_commits_and_returns_to_normal() {
    // Drive the same routing the event loop uses, end to end:
    // Normal -> Detail -> Form -> commit -> back.
    let mut state = TreeState::new(Node::new("Root"));
    assert!(matches!(state.mode, InputMode::Normal));

    dispatch_key(key(KeyCode::Enter), &mut state); // open Detail (a leaf)
    assert!(matches!(state.modal, Some(Modal::Detail(_))));

    dispatch_key(key(KeyCode::Char('i')), &mut state); // open the Form
    assert!(detail(&state).form().is_some());

    dispatch_key(key(KeyCode::Char('i')), &mut state); // enter Input
    assert_eq!(form_of(&state).focus, FormFocus::Input);

    type_text(&mut state, "!");
    dispatch_key(key(KeyCode::Enter), &mut state); // commit
    assert_eq!(state.root.name, "Root!");
    assert!(state.modified);

    dispatch_key(key(KeyCode::Esc), &mut state); // Form -> browse
    assert!(detail(&state).form().is_none());
    assert!(matches!(state.modal, Some(Modal::Detail(_))));

    dispatch_key(key(KeyCode::Esc), &mut state); // Detail -> Normal
    assert!(state.modal.is_none());
    assert!(matches!(state.mode, InputMode::Normal));
}

#[test]
fn typing_is_committed_on_input_exit() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut state);
    type_text(&mut state, "!");
    // The node is untouched while typing.
    assert_eq!(form_node(&state).name, "Root");
    handle_form(key(KeyCode::Enter), &mut state);
    assert_eq!(form_node(&state).name, "Root!");
    assert_eq!(form_of(&state).focus, FormFocus::Nav);
    assert!(state.modified);
}

#[test]
fn renaming_a_leaf_updates_the_visible_tree() {
    let leaf = Node::new("Old");
    let leaf_id = leaf.id;
    let root = Node::with_children("Root", vec![leaf]);
    let mut state = TreeState::new(root);
    state.expanded.insert(state.root.id);
    state.update_visible();
    assert_eq!(state.visible.len(), 2);

    state.select_by_id(leaf_id);
    let node = state.current_node_ref().expect("leaf exists").clone();
    state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));

    handle_form(key(KeyCode::Char('i')), &mut state);
    type_text(&mut state, "!");
    handle_form(key(KeyCode::Enter), &mut state);

    let visible_leaf = state
        .visible
        .iter()
        .find(|v| v.id == leaf_id)
        .expect("leaf stays visible");
    assert_eq!(visible_leaf.name, "Old!");

    // Undo and redo must also refresh the cached name.
    handle_form(key(KeyCode::Char('u')), &mut state);
    let visible_leaf = state.visible.iter().find(|v| v.id == leaf_id).unwrap();
    assert_eq!(visible_leaf.name, "Old");

    handle_form(key(KeyCode::Char('r')), &mut state);
    let visible_leaf = state.visible.iter().find(|v| v.id == leaf_id).unwrap();
    assert_eq!(visible_leaf.name, "Old!");
}

#[test]
fn leaving_input_unchanged_does_not_commit() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut state);
    handle_form(key(KeyCode::Esc), &mut state);
    assert_eq!(form_node(&state).name, "Root");
    assert!(!state.modified);
    assert_eq!(form_of(&state).history.len(), 1);
}

#[test]
fn undo_and_redo_restore_previous_state() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut state);
    type_text(&mut state, "!");
    handle_form(key(KeyCode::Enter), &mut state);
    assert_eq!(form_node(&state).name, "Root!");

    handle_form(key(KeyCode::Char('u')), &mut state);
    assert_eq!(form_node(&state).name, "Root");
    handle_form(key(KeyCode::Char('r')), &mut state);
    assert_eq!(form_node(&state).name, "Root!");
}

#[test]
fn undo_without_history_is_noop() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('u')), &mut state);
    assert_eq!(form_node(&state).name, "Root");
    assert!(detail(&state).form().is_some());
}

#[test]
fn new_edit_after_undo_truncates_redo_tail() {
    let mut state = editing_state();
    handle_form(key(KeyCode::Char('i')), &mut state);
    type_text(&mut state, "1");
    handle_form(key(KeyCode::Enter), &mut state);
    handle_form(key(KeyCode::Char('u')), &mut state);
    assert!(form_of(&state).can_redo());

    // A new commit discards the redo tail.
    handle_form(key(KeyCode::Char('i')), &mut state);
    type_text(&mut state, "2");
    handle_form(key(KeyCode::Enter), &mut state);
    assert_eq!(form_node(&state).name, "Root2");
    assert!(!form_of(&state).can_redo());
}

#[test]
fn add_form_starts_with_only_name_and_add_button() {
    let state = TreeState::new(Node::new("Root"));
    let form = FormState::new_add();
    assert_eq!(
        form.rows(&state.root),
        vec![FormRow::Name, FormRow::AddButton]
    );
}

#[test]
fn edit_form_rows_are_name_fields_and_add_button() {
    let mut node = Node::new("n");
    node.extra_fields.push(ExtraField {
        field_type: FieldType::Login,
        value: String::new(),
    });
    let state = TreeState::new(node);
    let form = FormState::new_edit(&state.root);
    assert_eq!(
        form.rows(&state.root),
        vec![FormRow::Name, FormRow::Extra(0), FormRow::AddButton]
    );
}

#[test]
fn add_form_first_commit_creates_the_node() {
    let mut state = TreeState::new(Node::new("Root"));
    state.add_position = AddPosition::Child;
    state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
    assert!(form_of(&state).is_adding());

    handle_form(key(KeyCode::Char('i')), &mut state);
    type_text(&mut state, "Child");
    handle_form(key(KeyCode::Enter), &mut state);

    let form = form_of(&state);
    assert!(!form.is_adding());
    let id = form.node_id.expect("node created");
    let created = state.root.find(id).expect("node in tree");
    assert_eq!(created.name, "Child");
    assert_eq!(state.visible[state.selected].id, id);
    assert!(state.modified);
}

#[test]
fn adding_field_via_picker_creates_node_and_enters_input() {
    let mut state = TreeState::new(Node::new("Root"));
    state.add_position = AddPosition::Child;
    state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
    select_row(&mut state, 1); // the + Add Field row

    handle_form(key(KeyCode::Enter), &mut state);
    assert!(detail(&state).is_select_field_type());

    // Custom is offered last; Description sits before it.
    let items = picker_items();
    assert_eq!(items.last(), Some(&PickerItem::Custom));

    let desc_index = items
        .iter()
        .position(|item| *item == PickerItem::Desc)
        .expect("Desc offered");
    detail_mut(&mut state).set_picker_selected(desc_index);
    handle_select_field_type(key(KeyCode::Enter), &mut state);

    let form = form_of(&state);
    assert!(!form.is_adding()); // the first commit created the node
    assert_eq!(form.focus, FormFocus::Input);
    assert_eq!(form.current_row(&state.root), FormRow::Extra(0));
    let node = form_node(&state);
    assert_eq!(node.extra_fields.len(), 1);
    assert_eq!(node.extra_fields[0].field_type.label(), "Description");
    assert_eq!(node.extra_fields[0].value, "");
    assert!(state.modified);
}

#[test]
fn password_is_masked_until_input_level() {
    let ef = ExtraField {
        field_type: FieldType::Password,
        value: "secret".to_string(),
    };
    // Nav: masked.
    let line = render_extra_field_line(&ef, &ef.value, 0, true, false);
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(!text.contains("secret"));
    assert!(text.contains(texts::PASSWORD_MASK));

    // Input: revealed.
    let line = render_extra_field_line(&ef, &ef.value, 0, true, true);
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains("secret"));
}

#[test]
fn field_cut_requires_two_d_presses_and_is_live() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 1); // "a"

    handle_form(key(KeyCode::Char('d')), &mut state);
    assert_eq!(form_of(&state).pending, FormPending::Cut);
    assert_eq!(values(&state).len(), 3); // not removed yet

    handle_form(key(KeyCode::Char('d')), &mut state);
    let form = form_of(&state);
    assert_eq!(form.pending, FormPending::None);
    assert_eq!(values(&state), vec!["b", "c"]);
    assert_eq!(form.field_cut_buffer.as_ref().unwrap().value, "a");
}

#[test]
fn field_cut_is_cancelled_by_other_keys() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 1);

    handle_form(key(KeyCode::Char('d')), &mut state);
    handle_form(key(KeyCode::Esc), &mut state);
    let form = form_of(&state);
    assert_eq!(form.pending, FormPending::None);
    assert_eq!(values(&state).len(), 3);
    assert!(detail(&state).form().is_some());
}

#[test]
fn field_cut_is_noop_on_name_row() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 0);
    handle_form(key(KeyCode::Char('d')), &mut state);
    assert_eq!(form_of(&state).pending, FormPending::None);
}

#[test]
fn field_paste_after_moves_field_down() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 1); // "a"

    // Cut "a", then select "b" and paste after it.
    handle_form(key(KeyCode::Char('d')), &mut state);
    handle_form(key(KeyCode::Char('d')), &mut state);
    select_row(&mut state, 1); // "b"
    handle_form(key(KeyCode::Char('p')), &mut state);
    assert_eq!(form_of(&state).pending, FormPending::Paste);
    handle_form(key(KeyCode::Char('p')), &mut state);

    let form = form_of(&state);
    assert_eq!(values(&state), vec!["b", "a", "c"]);
    assert!(form.field_cut_buffer.is_none());
}

#[test]
fn undo_clears_the_field_cut_buffer() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 1);
    handle_form(key(KeyCode::Char('d')), &mut state);
    handle_form(key(KeyCode::Char('d')), &mut state);
    assert!(form_of(&state).field_cut_buffer.is_some());

    handle_form(key(KeyCode::Char('u')), &mut state);
    assert!(form_of(&state).field_cut_buffer.is_none());
    assert_eq!(values(&state), vec!["a", "b", "c"]);
}

#[test]
fn paste_without_buffer_does_nothing() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 1);
    handle_form(key(KeyCode::Char('p')), &mut state);
    assert_eq!(form_of(&state).pending, FormPending::None);
    assert_eq!(values(&state).len(), 3);
}

#[test]
fn p_with_empty_buffer_stays_in_normal() {
    let mut state = TreeState::new(Node::new("Root"));
    handle_normal(key(KeyCode::Char('p')), &mut state);
    assert!(matches!(state.mode, InputMode::Normal));
}

#[test]
fn p_with_buffer_enters_pending_paste() {
    let mut state = TreeState::new(Node::new("Root"));
    state.cut_buffer = Some(Node::new("X"));
    handle_normal(key(KeyCode::Char('p')), &mut state);
    assert!(matches!(state.mode, InputMode::PendingPaste));
}

#[test]
fn confirm_delete_removes_the_field_and_clamps_selection() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 2); // second extra field
    detail_mut(&mut state).set_confirm_delete(1);
    handle_confirm_delete(key(KeyCode::Char('y')), &mut state);
    assert_eq!(values(&state), vec!["a", "c"]);
    assert!(form_of(&state).selected_row < form_of(&state).rows(&state.root).len());
}

#[test]
fn form_hint_offers_the_generator_only_while_editing_a_generated_field() {
    let mut password = editing_state_with_password("old");
    select_row(&mut password, 1);
    assert_eq!(
        form_hint(form_of(&password), &password.root),
        texts::HINT_FORM_EXTRA
    );
    handle_form(key(KeyCode::Char('i')), &mut password);
    assert_eq!(
        form_hint(form_of(&password), &password.root),
        texts::HINT_FORM_GENERATOR_INPUT
    );

    let mut login = editing_state_with_fields();
    select_row(&mut login, 1);
    assert_eq!(
        form_hint(form_of(&login), &login.root),
        texts::HINT_FORM_EXTRA
    );
    handle_form(key(KeyCode::Char('i')), &mut login);
    assert_eq!(
        form_hint(form_of(&login), &login.root),
        texts::HINT_FORM_GENERATOR_INPUT
    );

    // A Custom field has no generator, so its hint stays the plain input
    // one at the Input level.
    let mut custom = editing_state();
    custom.root.extra_fields.push(ExtraField {
        field_type: FieldType::Custom("URL".to_string()),
        value: String::new(),
    });
    let node = custom.root.clone();
    custom.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
    select_row(&mut custom, 1);
    handle_form(key(KeyCode::Char('i')), &mut custom);
    assert_eq!(
        form_hint(form_of(&custom), &custom.root),
        texts::HINT_FORM_INPUT
    );

    let mut name = editing_state();
    select_row(&mut name, 0);
    assert_eq!(form_hint(form_of(&name), &name.root), texts::HINT_FORM);
}

#[test]
fn form_extra_hint_advertises_paste_only_with_a_field_buffer() {
    let mut state = editing_state_with_fields();
    select_row(&mut state, 1);
    assert_eq!(
        form_hint(form_of(&state), &state.root),
        texts::HINT_FORM_EXTRA
    );

    handle_form(key(KeyCode::Char('d')), &mut state);
    handle_form(key(KeyCode::Char('d')), &mut state);
    assert_eq!(
        form_hint(form_of(&state), &state.root),
        texts::HINT_FORM_EXTRA_PASTE
    );
}
