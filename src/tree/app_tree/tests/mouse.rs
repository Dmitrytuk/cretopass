//! Mouse wheel and click hit-testing tests.

use super::*;

// ── Mouse wheel routing ──
/// A wheel event at the top-left; only `kind` matters to the router.
fn scroll(kind: event::MouseEventKind) -> event::MouseEvent {
    event::MouseEvent {
        kind,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn a_wheel_tick_moves_the_tree_selection() {
    let mut state = state_with_a_child();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    assert_eq!(state.selected, 0);

    handle_mouse(
        scroll(event::MouseEventKind::ScrollDown),
        &mut state,
        &mut terminal,
        &mut None,
        &mut 0,
    )
    .unwrap();
    assert_eq!(state.selected, 1, "the wheel selects the next row");

    handle_mouse(
        scroll(event::MouseEventKind::ScrollUp),
        &mut state,
        &mut terminal,
        &mut None,
        &mut 0,
    )
    .unwrap();
    assert_eq!(state.selected, 0, "the wheel selects the previous row");
}

#[test]
fn the_wheel_never_closes_help_or_an_alert() {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let scroll_down = scroll(event::MouseEventKind::ScrollDown);

    let mut help = TreeState::new(Node::new("Root"));
    help.modal = Some(Modal::Help(Help));
    handle_mouse(scroll_down, &mut help, &mut terminal, &mut None, &mut 0).unwrap();
    assert!(
        matches!(help.modal, Some(Modal::Help(_))),
        "the wheel must not dismiss Help"
    );

    let mut alert = TreeState::new(Node::new("Root"));
    alert.modal = Some(Modal::error("boom"));
    handle_mouse(scroll_down, &mut alert, &mut terminal, &mut None, &mut 0).unwrap();
    assert!(
        matches!(alert.modal, Some(Modal::Alert(_))),
        "the wheel must not dismiss an Alert"
    );
}

#[test]
fn the_wheel_is_a_noop_while_a_save_overlay_is_parked() {
    let mut state = state_with_a_child();
    state.save_exit = SaveExit::Blocked;
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    handle_mouse(
        scroll(event::MouseEventKind::ScrollDown),
        &mut state,
        &mut terminal,
        &mut None,
        &mut 0,
    )
    .unwrap();

    assert_eq!(state.selected, 0, "the parked overlay owns the screen");
    assert_eq!(state.save_exit, SaveExit::Blocked);
}

// ── Click hit-testing ──
/// Draws the tree screen so its renderer fills `state.hits`.
fn draw(state: &mut TreeState) {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| ui(f, state)).unwrap();
}

/// The first cell whose click resolves to `target` for `button`.
fn point_for(state: &TreeState, target: TreeHit, button: MouseButton) -> (u16, u16) {
    for y in 0..24u16 {
        for x in 0..80u16 {
            if state.hits.hit(x, y, button) == Some(target) {
                return (x, y);
            }
        }
    }
    panic!("no hit region for {target:?}");
}

#[test]
fn a_left_click_on_a_branch_toggles_it() {
    let mut state = state_with_a_child();
    draw(&mut state);
    let (x, y) = point_for(&state, TreeHit::Row(0), MouseButton::Left);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    handle_click(
        MouseButton::Left,
        x,
        y,
        &mut state,
        &mut terminal,
        &mut None,
        &mut 0,
    )
    .unwrap();

    assert_eq!(state.visible.len(), 1, "the Root branch collapsed");
    assert_eq!(state.selected, 0);
}

#[test]
fn a_left_click_on_a_leaf_only_selects_it() {
    let mut state = state_with_a_child();
    state.selected = 0;
    draw(&mut state);
    let (x, y) = point_for(&state, TreeHit::Row(1), MouseButton::Left);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    handle_click(
        MouseButton::Left,
        x,
        y,
        &mut state,
        &mut terminal,
        &mut None,
        &mut 0,
    )
    .unwrap();

    assert_eq!(state.selected, 1, "the click moved the selection");
    assert!(state.modal.is_none(), "a leaf click does not open Detail");
    assert_eq!(state.visible.len(), 2, "the click changed no expansion");
}

#[test]
fn a_right_click_on_a_row_opens_detail() {
    let mut state = state_with_a_child();
    draw(&mut state);
    let (x, y) = point_for(&state, TreeHit::Row(1), MouseButton::Right);

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    handle_click(
        MouseButton::Right,
        x,
        y,
        &mut state,
        &mut terminal,
        &mut None,
        &mut 0,
    )
    .unwrap();

    assert_eq!(state.selected, 1);
    assert!(matches!(state.modal, Some(Modal::Detail(_))));
}

#[test]
fn a_left_click_dismisses_help() {
    let mut state = state_with_a_child();
    state.modal = Some(Modal::Help(Help));

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    handle_click(
        MouseButton::Left,
        5,
        5,
        &mut state,
        &mut terminal,
        &mut None,
        &mut 0,
    )
    .unwrap();

    assert!(state.modal.is_none(), "a click dismisses Help anywhere");
}

// ── Outside-click dismissal (an outside click is the modal's Esc) ──
/// A cell guaranteed to fall outside `rect` on the 80×24 test screen.
fn outside_point(rect: Rect) -> (u16, u16) {
    for y in 0..24u16 {
        for x in 0..80u16 {
            if !crate::hitbox::point_in(rect, x, y) {
                return (x, y);
            }
        }
    }
    panic!("every cell is inside {rect:?}");
}

/// Drives one click through the same routing the event loop uses.
fn click(state: &mut TreeState, button: MouseButton, x: u16, y: u16) {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    handle_click(button, x, y, state, &mut terminal, &mut None, &mut 0).unwrap();
}

/// The panel the active modal drew, when a click outside it dismisses.
fn panel(state: &TreeState) -> Rect {
    state
        .modal
        .as_ref()
        .and_then(Modal::panel_rect)
        .expect("the active modal records a panel")
}

#[test]
fn a_click_outside_the_detail_panel_closes_it() {
    let mut state = state_with_a_child();
    state.selected = 1;
    state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
    draw(&mut state);

    let (x, y) = outside_point(panel(&state));
    click(&mut state, MouseButton::Left, x, y);

    assert!(state.modal.is_none(), "an outside click is Detail's Esc");
}

#[test]
fn a_click_inside_the_detail_panel_off_a_row_is_a_noop() {
    let mut state = state_with_a_child();
    state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
    draw(&mut state);

    // The panel's top-left cell is its border: inside the panel, but no
    // renderer recorded a row there.
    let rect = panel(&state);
    click(&mut state, MouseButton::Left, rect.x, rect.y);

    assert!(
        matches!(state.modal, Some(Modal::Detail(_))),
        "a click inside the panel must not close it"
    );
}

#[test]
fn a_right_click_outside_the_panel_is_ignored() {
    let mut state = state_with_a_child();
    state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
    draw(&mut state);

    let (x, y) = outside_point(panel(&state));
    click(&mut state, MouseButton::Right, x, y);

    assert!(
        matches!(state.modal, Some(Modal::Detail(_))),
        "only the left button dismisses"
    );
}

#[test]
fn a_click_outside_a_nested_popup_closes_only_that_popup() {
    let mut state = TreeState::new(Node::new("Root"));
    state.add_position = AddPosition::Child;
    state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
    select_row(&mut state, 1); // the + Add Field row
    handle_form(key(KeyCode::Enter), &mut state);
    assert!(detail(&state).is_select_field_type());
    draw(&mut state);

    let (x, y) = outside_point(panel(&state));
    click(&mut state, MouseButton::Left, x, y);

    assert!(
        matches!(state.modal, Some(Modal::Detail(_))),
        "one outside click unwinds one level, not the whole modal"
    );
    assert!(
        !detail(&state).is_select_field_type(),
        "the field-type picker closed"
    );
    assert!(
        detail(&state).form().is_some(),
        "the form underneath is still open"
    );
}

#[test]
fn a_click_outside_the_passphrase_confirmation_steps_back_one_level() {
    // The passwordless confirmation is a nested overlay: an outside click is
    // that screen's `Esc`, which returns to the fields rather than closing.
    let mut state = TreeState::new(Node::new("Root"));
    state.modal = Some(Modal::SharePassphrase(Box::new(SharePassphrase::new(
        Node::new("Work"),
        true,
        "/tmp/vault".to_string(),
    ))));
    handle_form(key(KeyCode::Enter), &mut state); // empty pair -> confirm
    assert!(detail_passphrase_is_confirming(&state));
    draw(&mut state);

    let (x, y) = outside_point(panel(&state));
    click(&mut state, MouseButton::Left, x, y);

    assert!(
        matches!(state.modal, Some(Modal::SharePassphrase(_))),
        "the password step stays open"
    );
    assert!(
        !detail_passphrase_is_confirming(&state),
        "the confirmation was dismissed one level"
    );
}

/// Whether the active SharePassphrase is showing its passwordless confirm.
fn detail_passphrase_is_confirming(state: &TreeState) -> bool {
    match &state.modal {
        Some(Modal::SharePassphrase(prompt)) => prompt.is_confirming_empty(),
        other => panic!("expected the share passphrase step, got {other:?}"),
    }
}

#[test]
fn a_click_outside_the_export_wizard_closes_it() {
    let mut state = state_with_a_child();
    state.modal = Some(Modal::Export(ExportWizard::new(Node::new("Work"))));
    draw(&mut state);

    let (x, y) = outside_point(panel(&state));
    click(&mut state, MouseButton::Left, x, y);

    assert!(
        state.modal.is_none(),
        "an outside click is the export wizard's Esc"
    );
}

#[test]
fn a_click_inside_the_export_wizard_panel_off_a_row_is_a_noop() {
    let mut state = state_with_a_child();
    state.modal = Some(Modal::Export(ExportWizard::new(Node::new("Work"))));
    draw(&mut state);

    let rect = panel(&state);
    click(&mut state, MouseButton::Left, rect.x, rect.y);

    assert!(
        matches!(state.modal, Some(Modal::Export(_))),
        "a click inside the panel must not close the wizard"
    );
}

#[test]
fn a_click_outside_the_delete_confirmation_declines() {
    // Acceptance criterion 3 on the Tree: an outside click on the destructive
    // prompt denies it and deletes nothing.
    let mut state = editing_state_with_fields();
    detail_mut(&mut state).set_confirm_delete(0);
    draw(&mut state);

    let (x, y) = outside_point(panel(&state));
    click(&mut state, MouseButton::Left, x, y);

    assert!(
        matches!(state.modal, Some(Modal::Detail(_))),
        "the form stays open behind the prompt"
    );
    assert!(
        !detail(&state).is_confirm_delete(),
        "an outside click declines the prompt"
    );
    assert_eq!(
        values(&state).len(),
        3,
        "an outside click declines: no field is deleted"
    );
}

#[test]
fn a_click_just_past_the_panel_edge_closes_it() {
    // The point `point_in` rules out is the first cell past the panel's
    // right edge, where an off-by-one would hide.
    let mut state = state_with_a_child();
    state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
    draw(&mut state);

    let rect = panel(&state);
    click(&mut state, MouseButton::Left, rect.right(), rect.y + 1);

    assert!(
        state.modal.is_none(),
        "a near-miss past the edge still counts as outside"
    );
}

#[test]
fn a_click_before_the_first_frame_keeps_the_modal() {
    // `rect` is `None` until the renderer runs, so a modal that was never
    // drawn cannot be dismissed by a stray click.
    let mut state = state_with_a_child();
    state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
    assert!(state.modal.as_ref().unwrap().panel_rect().is_none());

    click(&mut state, MouseButton::Left, 0, 0);

    assert!(
        matches!(state.modal, Some(Modal::Detail(_))),
        "an unrendered modal must survive an outside click"
    );
}
