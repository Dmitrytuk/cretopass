//! Password- and login-word generator tests (`Ctrl+G`).

use super::*;

fn password_value(state: &TreeState) -> &str {
    &form_node(state).extra_fields[0].value
}

fn generator_modal(state: &TreeState) -> &GeneratorModal {
    detail(state)
        .generator()
        .expect("a generator popup is open")
}

fn open_generator(state: &mut TreeState) {
    select_row(state, 1);
    if form_of(state).focus == FormFocus::Nav {
        handle_form(key(KeyCode::Char('i')), state);
    }
    handle_form(ctrl(KeyCode::Char('g')), state);
    assert!(detail(state).generator().is_some());
}

fn move_generator_to(state: &mut TreeState, target: usize) {
    while generator_modal(state).selected < target {
        dispatch_key(key(KeyCode::Char('j')), state);
    }
    while generator_modal(state).selected > target {
        dispatch_key(key(KeyCode::Char('k')), state);
    }
}

fn set_generator_length(state: &mut TreeState, digits: &str) {
    while !generator_modal(state).length_input.is_empty() {
        dispatch_key(key(KeyCode::Backspace), state);
    }
    for digit in digits.chars() {
        dispatch_key(key(KeyCode::Char(digit)), state);
    }
}

fn press_generate(state: &mut TreeState) {
    move_generator_to(state, GENERATOR_ROW_GENERATE);
    dispatch_key(key(KeyCode::Enter), state);
}

#[test]
fn generator_ctrl_g_is_bound_only_while_editing_a_password_field() {
    // `Ctrl+G` is unbound at the Nav level, even on a password row.
    let mut nav = editing_state_with_password("old");
    select_row(&mut nav, 1);
    handle_form(ctrl(KeyCode::Char('g')), &mut nav);
    assert!(detail(&nav).form().is_some());

    // At the Input level it opens only when the focused field carries a
    // generator: Login and Name rows are no-ops.
    let mut login = editing_state_with_fields();
    select_row(&mut login, 1);
    handle_form(key(KeyCode::Char('i')), &mut login);
    handle_form(ctrl(KeyCode::Char('g')), &mut login);
    assert!(detail(&login).form().is_some());

    let mut name = editing_state();
    select_row(&mut name, 0);
    handle_form(key(KeyCode::Char('i')), &mut name);
    handle_form(ctrl(KeyCode::Char('g')), &mut name);
    assert!(detail(&name).form().is_some());

    let mut password = editing_state_with_password("old");
    open_generator(&mut password);
}

#[test]
fn generator_generate_writes_commits_and_returns_to_input_form() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    set_generator_length(&mut state, "12");
    press_generate(&mut state);

    assert!(detail(&state).form().is_some());
    assert_eq!(form_of(&state).focus, FormFocus::Input);
    let value = password_value(&state);
    assert_eq!(value.chars().count(), 12);
    assert!(value.chars().all(|c| c.is_ascii_graphic()));
    assert!(state.modified);
    assert_eq!(form_of(&state).history.len(), 2);
    assert_eq!(state.generator.length.value(), 12);

    // The scratch buffer mirrors the generated value, so leaving the input
    // level keeps the password instead of restoring the old one.
    let generated = value.to_string();
    assert_eq!(
        form_of(&state).scratch.as_ref().map(|s| s.text.as_str()),
        Some(generated.as_str())
    );
    handle_form(key(KeyCode::Esc), &mut state);
    assert_eq!(form_of(&state).focus, FormFocus::Nav);
    assert_eq!(password_value(&state), generated);
}

#[test]
fn generated_password_is_undoable() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    press_generate(&mut state);
    assert_ne!(password_value(&state), "old");

    // Generation leaves the form at the Input level, where `u` is plain
    // text; leave it first so `u` reaches the Nav-level undo.
    handle_form(key(KeyCode::Esc), &mut state);
    handle_form(key(KeyCode::Char('u')), &mut state);
    assert_eq!(password_value(&state), "old");
}

#[test]
fn generator_esc_leaves_the_value_untouched() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    dispatch_key(key(KeyCode::Esc), &mut state);

    assert!(detail(&state).form().is_some());
    assert_eq!(password_value(&state), "old");
    assert!(!state.modified);
    assert_eq!(form_of(&state).history.len(), 1);
}

#[test]
fn generator_out_of_range_length_shows_an_error_and_does_not_write() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    set_generator_length(&mut state, "999");
    press_generate(&mut state);

    assert!(detail(&state).generator().is_some());
    assert!(!detail(&state).is_symbol_checklist());
    assert_eq!(
        generator_modal(&state).error,
        Some(generator::PasswordLengthError::OutOfRange)
    );
    let value = &generator_modal(&state).form.node(&state.root).extra_fields[0].value;
    assert_eq!(value, "old");
    assert!(!state.modified);
}

#[test]
fn generator_empty_length_shows_the_whole_number_error() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    set_generator_length(&mut state, "");
    press_generate(&mut state);

    assert_eq!(
        generator_modal(&state).error,
        Some(generator::PasswordLengthError::NotANumber)
    );
    let text = rendered_text(&mut state);
    assert!(
        text.contains("length must be a whole number"),
        "the modal must render the whole-number error: {text:?}"
    );
}

#[test]
fn generator_out_of_range_length_shows_the_range_error() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    set_generator_length(&mut state, "999");
    press_generate(&mut state);

    assert_eq!(
        generator_modal(&state).error,
        Some(generator::PasswordLengthError::OutOfRange)
    );
    let text = rendered_text(&mut state);
    assert!(
        text.contains("length must be between 4 and 128"),
        "the modal must render the range error: {text:?}"
    );
}

#[test]
fn generator_digits_replace_the_seed_and_edit_at_the_caret() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);

    // The seeded "10" is replaced by the first digits typed.
    dispatch_key(key(KeyCode::Char('1')), &mut state);
    dispatch_key(key(KeyCode::Char('2')), &mut state);
    assert_eq!(generator_modal(&state).length_input, "12");
    assert_eq!(generator_modal(&state).length_cursor, 2);

    // The caret can be moved and a digit inserted mid-string.
    dispatch_key(key(KeyCode::Left), &mut state);
    assert_eq!(generator_modal(&state).length_cursor, 1);
    dispatch_key(key(KeyCode::Char('0')), &mut state);
    assert_eq!(generator_modal(&state).length_input, "102");

    // Home / End move to the edges; Delete removes at the caret.
    dispatch_key(key(KeyCode::Home), &mut state);
    assert_eq!(generator_modal(&state).length_cursor, 0);
    dispatch_key(key(KeyCode::Delete), &mut state);
    assert_eq!(generator_modal(&state).length_input, "02");
    dispatch_key(key(KeyCode::End), &mut state);
    assert_eq!(generator_modal(&state).length_cursor, 2);

    // At most three digits are kept.
    dispatch_key(key(KeyCode::Char('7')), &mut state);
    dispatch_key(key(KeyCode::Char('8')), &mut state);
    assert_eq!(generator_modal(&state).length_input, "027");
}

#[test]
fn generator_length_backspace_on_fresh_seed_clears_it() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);

    dispatch_key(key(KeyCode::Backspace), &mut state);
    assert_eq!(generator_modal(&state).length_input, "");
    assert_eq!(generator_modal(&state).length_cursor, 0);

    dispatch_key(key(KeyCode::Char('4')), &mut state);
    dispatch_key(key(KeyCode::Char('2')), &mut state);
    assert_eq!(generator_modal(&state).length_input, "42");
}

#[test]
fn generator_digits_and_caret_keys_only_edit_the_length_row() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    move_generator_to(&mut state, GENERATOR_ROW_GENERATE);

    for code in [
        KeyCode::Char('5'),
        KeyCode::Backspace,
        KeyCode::Delete,
        KeyCode::Left,
        KeyCode::Home,
    ] {
        dispatch_key(key(code), &mut state);
    }
    assert_eq!(generator_modal(&state).length_input, "10");
    assert_eq!(generator_modal(&state).length_cursor, 2);
}

#[test]
fn generator_enter_on_length_moves_to_symbols_row() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_LENGTH);

    dispatch_key(key(KeyCode::Enter), &mut state);
    assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_SYMBOLS);
}

#[test]
fn generator_symbols_summary_toggles_all() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);

    dispatch_key(key(KeyCode::Char(' ')), &mut state);
    assert!(generator_modal(&state).params.symbols.is_empty());
    dispatch_key(key(KeyCode::Char(' ')), &mut state);
    assert_eq!(
        generator_modal(&state).params.symbols,
        generator::SymbolSet::all()
    );
}

#[test]
fn symbol_checklist_moves_in_a_grid_and_toggles() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);
    dispatch_key(key(KeyCode::Enter), &mut state);
    assert!(detail(&state).is_symbol_checklist());

    dispatch_key(key(KeyCode::Char(' ')), &mut state);
    assert!(!generator_modal(&state).params.symbols.contains('!'));
    dispatch_key(key(KeyCode::Char('l')), &mut state);
    assert_eq!(generator_modal(&state).selected, 1);
    dispatch_key(key(KeyCode::Char('j')), &mut state);
    assert_eq!(generator_modal(&state).selected, 4);
    dispatch_key(key(KeyCode::Char(' ')), &mut state);
    assert!(!generator_modal(&state).params.symbols.contains('%'));

    dispatch_key(key(KeyCode::Esc), &mut state);
    assert!(detail(&state).generator().is_some());
    assert!(!detail(&state).is_symbol_checklist());
    assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_SYMBOLS);
}

#[test]
fn generator_with_all_symbols_off_generates_alphanumerics() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);
    dispatch_key(key(KeyCode::Char(' ')), &mut state);
    press_generate(&mut state);

    assert!(detail(&state).form().is_some());
    let value = password_value(&state);
    assert!(value.chars().all(|c| c.is_ascii_alphanumeric()));
    assert!(value.chars().any(|c| c.is_ascii_digit()));
}

#[test]
fn generator_settings_are_remembered_for_the_next_open() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    set_generator_length(&mut state, "12");
    press_generate(&mut state);

    open_generator(&mut state);
    assert_eq!(generator_modal(&state).length_input, "12");
}

fn open_login_row(state: &mut TreeState) {
    select_row(state, 1);
    if form_of(state).focus == FormFocus::Nav {
        handle_form(key(KeyCode::Char('i')), state);
    }
}

fn generate_login_word(state: &mut TreeState) {
    assert_eq!(form_of(state).focus, FormFocus::Input);
    handle_form(ctrl(KeyCode::Char('g')), state);
}

#[test]
fn ctrl_g_on_a_login_row_writes_a_word_without_opening_a_modal() {
    let mut state = editing_state_with_fields();
    open_login_row(&mut state);
    generate_login_word(&mut state);

    // No generator popup: the form is still active at the Input level.
    assert!(detail(&state).form().is_some());
    assert_eq!(form_of(&state).focus, FormFocus::Input);

    let value = values(&state)[0];
    assert_ne!(value, "a");
    assert!(value.chars().all(|c| c.is_ascii_lowercase()));
    assert!((4..=13).contains(&value.chars().count()));
    assert!(state.modified);
    // The scratch buffer mirrors the value, so leaving Input keeps it.
    assert_eq!(
        form_of(&state).scratch.as_ref().map(|s| s.text.as_str()),
        Some(value)
    );
    let generated = value.to_string();
    handle_form(key(KeyCode::Esc), &mut state);
    assert_eq!(values(&state)[0], generated);
}

#[test]
fn login_reroll_coalesces_into_one_undo_step() {
    let mut state = editing_state_with_fields();
    open_login_row(&mut state);
    generate_login_word(&mut state);
    assert_eq!(form_of(&state).history.len(), 2);

    generate_login_word(&mut state);
    // The reroll replaced the previous generation instead of stacking.
    assert_eq!(form_of(&state).history.len(), 2);
    assert!(values(&state)[0].chars().all(|c| c.is_ascii_lowercase()));

    // One undo restores the pre-generation value.
    handle_form(key(KeyCode::Esc), &mut state);
    handle_form(key(KeyCode::Char('u')), &mut state);
    assert_eq!(values(&state)[0], "a");
}

#[test]
fn ctrl_g_outside_a_generated_field_is_a_noop() {
    // Name row at the Input level.
    let mut state = editing_state();
    select_row(&mut state, 0);
    handle_form(key(KeyCode::Char('i')), &mut state);
    handle_form(ctrl(KeyCode::Char('g')), &mut state);
    assert!(!state.modified);
    assert_eq!(form_of(&state).history.len(), 1);

    // A Login row at the Nav level: `Ctrl+G` is unbound there.
    let mut state = editing_state_with_fields();
    select_row(&mut state, 1);
    handle_form(ctrl(KeyCode::Char('g')), &mut state);
    assert_eq!(form_of(&state).focus, FormFocus::Nav);
    assert!(!state.modified);
    assert_eq!(values(&state)[0], "a");
}

#[test]
fn password_reroll_also_coalesces_into_one_undo_step() {
    let mut state = editing_state_with_password("old");
    open_generator(&mut state);
    press_generate(&mut state);
    assert_eq!(form_of(&state).history.len(), 2);

    open_generator(&mut state);
    press_generate(&mut state);
    assert_eq!(form_of(&state).history.len(), 2);

    handle_form(key(KeyCode::Esc), &mut state);
    handle_form(key(KeyCode::Char('u')), &mut state);
    assert_eq!(password_value(&state), "old");
}
