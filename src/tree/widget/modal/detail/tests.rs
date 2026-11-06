//! Unit tests for the Detail modal, grouped by surface (browse, paste, clicks).

use super::*;
use crate::tree::node::{ExtraField, FieldType};
use crate::tree::widget::FormFocus;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// A tree whose root has three extra fields, opened in the Detail modal.
fn detail_with_fields() -> (Detail, TreeState) {
    let mut node = Node::new("Root");
    for value in ["a", "b", "c"] {
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: value.to_string(),
        });
    }
    (Detail::browse(), TreeState::new(node))
}

#[test]
fn tab_wraps_forward_and_backtab_wraps_backward() {
    let (mut detail, mut state) = detail_with_fields();
    let _ = detail.handle_key(key(KeyCode::Tab), &mut state);
    assert_eq!(detail.selected, 1);
    let _ = detail.handle_key(key(KeyCode::Tab), &mut state);
    let _ = detail.handle_key(key(KeyCode::Tab), &mut state);
    assert_eq!(detail.selected, 0); // wrapped past the last field

    let _ = detail.handle_key(key(KeyCode::BackTab), &mut state);
    assert_eq!(detail.selected, 2); // wrapped past the first field
}

#[test]
fn nav_down_clamps_at_last_field() {
    let (mut detail, mut state) = detail_with_fields();
    for _ in 0..5 {
        let _ = detail.handle_key(key(KeyCode::Char('j')), &mut state);
    }
    assert_eq!(detail.selected, 2);
}

#[test]
fn nav_up_clamps_at_first_field() {
    let (mut detail, mut state) = detail_with_fields();
    let _ = detail.handle_key(key(KeyCode::Char('k')), &mut state);
    assert_eq!(detail.selected, 0);
}

#[test]
fn tab_is_safe_without_fields() {
    let mut detail = Detail::browse();
    let mut state = TreeState::new(Node::new("Root"));
    let _ = detail.handle_key(key(KeyCode::Tab), &mut state);
    let _ = detail.handle_key(key(KeyCode::BackTab), &mut state);
    assert_eq!(detail.selected, 0);
}

#[test]
fn esc_asks_to_close() {
    let (mut detail, mut state) = detail_with_fields();
    assert!(matches!(
        detail.handle_key(key(KeyCode::Esc), &mut state),
        Some(TreeAction::Close)
    ));
}

#[test]
fn i_opens_the_edit_form() {
    let (mut detail, mut state) = detail_with_fields();
    assert!(
        detail
            .handle_key(key(KeyCode::Char('i')), &mut state)
            .is_none()
    );
    assert!(detail.form().is_some());
}

#[test]
fn y_copies_the_selected_value() {
    let (mut detail, mut state) = detail_with_fields();
    match detail.handle_key(key(KeyCode::Char('y')), &mut state) {
        Some(TreeAction::CopyToClipboard { text }) => assert_eq!(text, "a"),
        _ => panic!("expected a clipboard copy"),
    }
}

#[test]
fn an_empty_value_asks_for_no_copy() {
    let mut node = Node::new("Root");
    node.extra_fields.push(ExtraField {
        field_type: FieldType::Login,
        value: String::new(),
    });
    let mut detail = Detail::browse();
    let mut state = TreeState::new(node);
    assert!(
        detail
            .handle_key(key(KeyCode::Char('y')), &mut state)
            .is_none(),
        "an empty value must not reach the clipboard"
    );
}

#[test]
fn no_field_asks_for_no_copy() {
    let mut detail = Detail::browse();
    let mut state = TreeState::new(Node::new("Root"));
    assert!(
        detail
            .handle_key(key(KeyCode::Char('y')), &mut state)
            .is_none()
    );
}

// ── Clipboard paste per surface ──

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

fn editing_password(value: &str) -> (Detail, TreeState) {
    let mut node = Node::new("Root");
    node.extra_fields.push(ExtraField {
        field_type: FieldType::Password,
        value: value.to_string(),
    });
    let detail = Detail::editing(&node);
    (detail, TreeState::new(node))
}

fn scratch_text(detail: &Detail) -> Option<&str> {
    detail
        .form()
        .and_then(|form| form.scratch.as_ref())
        .map(|scratch| scratch.text.as_str())
}

#[test]
fn form_paste_joins_lines_into_the_input_scratch() {
    let node = Node::new("Root");
    let mut detail = Detail::editing(&node);
    let mut state = TreeState::new(node);
    let _ = detail.handle_key(key(KeyCode::Char('i')), &mut state); // Nav -> Input

    detail.handle_paste("ab\r\ncd\nef", &mut state);

    assert_eq!(scratch_text(&detail), Some("Rootabcdef"));
}

#[test]
fn a_paste_that_cleans_away_to_nothing_changes_nothing() {
    let node = Node::new("Root");
    let mut detail = Detail::editing(&node);
    let mut state = TreeState::new(node);
    let _ = detail.handle_key(key(KeyCode::Char('i')), &mut state);

    detail.handle_paste("\r\n", &mut state);

    assert_eq!(scratch_text(&detail), Some("Root"));
}

#[test]
fn custom_field_name_paste_appends_single_line_text() {
    let mut detail = Detail::browse();
    let mut state = TreeState::new(Node::new("Root"));
    detail.popup = DetailPopup::CustomFieldName {
        input: "Web".to_string(),
    };

    detail.handle_paste("Site\r\nLogin", &mut state);

    match &detail.popup {
        DetailPopup::CustomFieldName { input } => assert_eq!(input, "WebSiteLogin"),
        other => panic!("expected the custom-name prompt, got {other:?}"),
    }
}

#[test]
fn generator_length_paste_keeps_only_digits() {
    let (mut detail, mut state) = editing_password("old");
    detail.form_mut().select_row(&state.root, 1); // the Password field
    let _ = detail.handle_key(key(KeyCode::Char('i')), &mut state); // Nav -> Input
    let _ = detail.handle_key(ctrl(KeyCode::Char('g')), &mut state); // open the generator

    detail.handle_paste("1a2b3", &mut state);

    match &detail.popup {
        DetailPopup::GeneratePassword(modal) => assert_eq!(modal.length_input, "123"),
        other => panic!("expected the password generator, got {other:?}"),
    }
}

#[test]
fn generator_length_paste_is_capped_at_three_digits() {
    let (mut detail, mut state) = editing_password("old");
    detail.form_mut().select_row(&state.root, 1);
    let _ = detail.handle_key(key(KeyCode::Char('i')), &mut state);
    let _ = detail.handle_key(ctrl(KeyCode::Char('g')), &mut state);

    detail.handle_paste("12345", &mut state);

    match &detail.popup {
        DetailPopup::GeneratePassword(modal) => assert_eq!(modal.length_input, "123"),
        other => panic!("expected the password generator, got {other:?}"),
    }
}

#[test]
fn generator_paste_on_another_row_changes_nothing() {
    let (mut detail, mut state) = editing_password("old");
    detail.form_mut().select_row(&state.root, 1);
    let _ = detail.handle_key(key(KeyCode::Char('i')), &mut state);
    let _ = detail.handle_key(ctrl(KeyCode::Char('g')), &mut state);
    // Move off the length row onto the symbols row.
    let _ = detail.handle_key(key(KeyCode::Char('j')), &mut state);
    let before = match &detail.popup {
        DetailPopup::GeneratePassword(modal) => modal.length_input.clone(),
        other => panic!("expected the password generator, got {other:?}"),
    };

    detail.handle_paste("123", &mut state);

    match &detail.popup {
        DetailPopup::GeneratePassword(modal) => assert_eq!(modal.length_input, before),
        other => panic!("expected the password generator, got {other:?}"),
    }
}

// ── Click hit-testing ──

/// Draws the modal so its renderer fills `detail.clicks`.
fn draw(detail: &mut Detail, state: &TreeState) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24))
        .expect("a test terminal");
    terminal
        .draw(|frame| detail.render(frame, frame.area(), state))
        .expect("a test draw");
}

/// The first cell whose click resolves to `target` for `button`.
fn point_for(
    clicks: &ClickSurface<DetailHit>,
    target: DetailHit,
    button: MouseButton,
) -> (u16, u16) {
    for y in 0..24u16 {
        for x in 0..80u16 {
            if clicks.target_at(button, x, y) == Some(target) {
                return (x, y);
            }
        }
    }
    panic!("no hit region for {target:?}");
}

#[test]
fn a_click_selects_a_field_and_a_repeat_opens_the_form() {
    let (mut detail, mut state) = detail_with_fields();
    draw(&mut detail, &state);
    let (x, y) = point_for(&detail.clicks, DetailHit::Field(1), MouseButton::Left);

    // The first click only moves the selection.
    assert!(
        detail
            .handle_click(MouseButton::Left, x, y, &mut state)
            .is_none()
    );
    assert_eq!(detail.selected, 1);
    assert!(detail.form().is_none());

    // The repeat click opens the Form on that field.
    let _ = detail.handle_click(MouseButton::Left, x, y, &mut state);
    assert!(detail.form().is_some(), "a repeat click opens the form");
}

#[test]
fn a_right_click_copies_the_field_value() {
    let (mut detail, mut state) = detail_with_fields();
    draw(&mut detail, &state);
    let (x, y) = point_for(&detail.clicks, DetailHit::Field(0), MouseButton::Right);

    match detail.handle_click(MouseButton::Right, x, y, &mut state) {
        Some(TreeAction::CopyToClipboard { text }) => assert_eq!(text, "a"),
        other => panic!("expected a clipboard copy, got {other:?}"),
    }
}

#[test]
fn a_click_on_a_checklist_cell_toggles_it() {
    let (mut detail, mut state) = editing_password("old");
    detail.form_mut().select_row(&state.root, 1);
    let _ = detail.handle_key(key(KeyCode::Char('i')), &mut state);
    let _ = detail.handle_key(ctrl(KeyCode::Char('g')), &mut state);
    // Symbols row -> checklist.
    let _ = detail.handle_key(key(KeyCode::Char('j')), &mut state);
    let _ = detail.handle_key(key(KeyCode::Enter), &mut state);
    assert!(detail.is_symbol_checklist());

    draw(&mut detail, &state);
    let before = match &detail.popup {
        DetailPopup::SymbolChecklist(modal) => modal.params.symbols.len(),
        other => panic!("expected the symbol checklist, got {other:?}"),
    };
    let (x, y) = point_for(&detail.clicks, DetailHit::SymbolCell(0), MouseButton::Left);
    let _ = detail.handle_click(MouseButton::Left, x, y, &mut state);

    match &detail.popup {
        DetailPopup::SymbolChecklist(modal) => {
            assert_ne!(modal.params.symbols.len(), before, "the cell toggled");
        }
        other => panic!("expected the symbol checklist, got {other:?}"),
    }
}

#[test]
fn a_click_on_a_form_row_selects_then_enters() {
    let (mut detail, mut state) = detail_with_fields();
    // Open the form for the selected node.
    let _ = detail.handle_key(key(KeyCode::Char('i')), &mut state);
    draw(&mut detail, &state);

    // Rows are Name(0), Extra(0)(1), Extra(1)(2), Extra(2)(3), AddButton(4).
    let (x, y) = point_for(&detail.clicks, DetailHit::FormRow(2), MouseButton::Left);
    assert!(
        detail
            .handle_click(MouseButton::Left, x, y, &mut state)
            .is_none()
    );
    assert_eq!(detail.form().unwrap().selected_row, 2);

    let _ = detail.handle_click(MouseButton::Left, x, y, &mut state);
    assert_eq!(detail.form().unwrap().focus, FormFocus::Input);
}

#[test]
fn a_short_card_records_no_field_hits_below_it() {
    // A 30-field node on a 6-row terminal: the card is clamped, so the extra
    // rows are never drawn — and must not be clickable either, or a
    // right-click on the status bar would copy an off-screen secret.
    let mut node = Node::new("Root");
    for i in 0..30 {
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: format!("v{i}"),
        });
    }
    let mut detail = Detail::browse();
    let state = TreeState::new(node);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 6)).expect("a test terminal");
    terminal
        .draw(|frame| detail.render(frame, frame.area(), &state))
        .expect("a test draw");

    for x in 0..80u16 {
        for y in 0..6u16 {
            assert!(
                detail.clicks.target_at(MouseButton::Right, x, y).is_none() || y < 5,
                "a field was clickable outside the card at ({x}, {y})"
            );
        }
    }
}
