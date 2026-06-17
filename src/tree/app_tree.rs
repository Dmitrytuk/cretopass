use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use ratatui::{backend::Backend, Terminal};
use crate::tree::node::{ExtraField, FieldType};
use crate::tree::widget::{InputMode, TreeState, TreeWidget};

pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
) -> io::Result<()> {
    loop {
        if state.quit {
            break Ok(());
        }

        terminal.draw(|f| ui(f, state))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match &state.mode {
                InputMode::Normal => handle_normal(key, state),
                InputMode::Search => handle_search(key, state),
                InputMode::AwaitingDd => {
                    state.mode = InputMode::Normal;
                    if key.code == KeyCode::Char('d') {
                        state.cut_selected();
                    }
                }
                InputMode::Adding { .. } => handle_adding(key, state),
                InputMode::Editing { .. } => handle_editing(key, state),
                InputMode::SelectFieldType { .. } => handle_select_field_type(key, state),
                InputMode::CustomFieldName { .. } => handle_custom_field_name(key, state),
                InputMode::ConfirmDeleteField { .. } => handle_confirm_delete(key, state),
                InputMode::ConfirmDiscardChanges { .. } => handle_confirm_discard(key, state),
                InputMode::Detail { .. } => handle_detail(key, state),
            }
        }
    }
}

fn is_leaf_selected(state: &TreeState) -> bool {
    state
        .visible
        .get(state.selected)
        .map(|v| !v.is_parent)
        .unwrap_or(false)
}

fn handle_normal(key: event::KeyEvent, state: &mut TreeState) {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.quit = true;
        return;
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            state.quit = true;
        }
        KeyCode::Char('H') => {
            state.return_to_menu = true;
            state.quit = true;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            state.select_next();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.select_prev();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail { selected: 0, alert: None };
            } else {
                state.expand_collapse(true);
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            state.expand_collapse(false);
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.mode = InputMode::Detail { selected: 0, alert: None };
        }
        KeyCode::Enter => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail { selected: 0, alert: None };
            } else {
                state.toggle_expand();
            }
        }
        KeyCode::Char(' ') => {
            state.toggle_expand();
        }
        KeyCode::Char('d') => {
            state.mode = InputMode::AwaitingDd;
        }
        KeyCode::Char('p') => {
            state.paste_sibling();
        }
        KeyCode::Char('P') => {
            state.paste_child();
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            state.mode = InputMode::Detail { selected: 0, alert: None };
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            state.mode = InputMode::Adding {
                name: String::new(),
                desc: String::new(),
                extra_fields: Vec::new(),
                field: 0,
                cursor: 0,
            };
        }
        KeyCode::Char('/') => {
            state.mode = InputMode::Search;
        }
        KeyCode::Esc => {
            state.clear_search();
        }
        _ => {}
    }
}

fn handle_search(key: event::KeyEvent, state: &mut TreeState) {
    match key.code {
        KeyCode::Enter => {
            state.mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            state.clear_search();
            state.mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            let mut q = state.search_query.clone();
            q.pop();
            state.set_search(q);
        }
        KeyCode::Char(c) => {
            let mut q = state.search_query.clone();
            q.push(c);
            state.set_search(q);
        }
        _ => {}
    }
}

fn total_fields(extra_fields: &[ExtraField]) -> usize {
    3 + extra_fields.len()
}

fn extra_field_index(field: usize) -> Option<usize> {
    if field >= 2 {
        Some(field - 2)
    } else {
        None
    }
}

fn active_len_for_field(name: &str, desc: &str, extra_fields: &[ExtraField], field: usize) -> usize {
    match field {
        0 => name.len(),
        1 => desc.len(),
        f => {
            if let Some(ef_idx) = f.checked_sub(2) {
                if ef_idx < extra_fields.len() {
                    extra_fields[ef_idx].value.len()
                } else {
                    0
                }
            } else {
                0
            }
        }
    }
}

fn cursor_on_new_field(field: usize, name: &str, desc: &str, extra_fields: &[ExtraField]) -> usize {
    match field {
        0 => name.len(),
        1 => desc.len(),
        f => {
            if let Some(ef_idx) = f.checked_sub(2) {
                if ef_idx < extra_fields.len() {
                    extra_fields[ef_idx].value.len()
                } else {
                    0
                }
            } else {
                0
            }
        }
    }
}

fn build_editing_mode(
    node_id: Option<uuid::Uuid>,
    name: String,
    desc: String,
    extra_fields: Vec<ExtraField>,
    field: usize,
    cursor: usize,
    is_adding: bool,
) -> InputMode {
    if is_adding {
        InputMode::Adding { name, desc, extra_fields, field, cursor }
    } else if let Some(nid) = node_id {
        InputMode::Editing { node_id: nid, name, desc, extra_fields, field, cursor }
    } else {
        InputMode::Adding { name, desc, extra_fields, field, cursor }
    }
}

fn handle_adding(key: event::KeyEvent, state: &mut TreeState) {
    let (name, desc, extra_fields, field, cursor) = match &state.mode {
        InputMode::Adding { name, desc, extra_fields, field, cursor } => {
            (name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor)
        }
        _ => return,
    };

    let total = total_fields(&extra_fields);
    let is_on_extra = extra_field_index(field).map_or(false, |i| i < extra_fields.len());
    let is_on_add_button = field == 2 + extra_fields.len();
    let active_len = active_len_for_field(&name, &desc, &extra_fields, field);

    match key.code {
        KeyCode::Delete if is_on_extra => {
            state.mode = InputMode::ConfirmDeleteField {
                node_id: None,
                name, desc, extra_fields, field, cursor,
                target_index: field - 2,
            };
        }
        KeyCode::Enter if is_on_add_button => {
            state.mode = InputMode::SelectFieldType {
                node_id: None,
                name, desc, extra_fields, field, cursor,
                selected_type: 0,
            };
        }
        KeyCode::Enter => {
            state.add_child_to_selected(name, desc, extra_fields);
            state.mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            if adding_has_changes(&name, &desc, &extra_fields) {
                state.mode = InputMode::ConfirmDiscardChanges {
                    node_id: None, name, desc, extra_fields, field, cursor,
                };
            } else {
                state.mode = InputMode::Normal;
            }
        }
        KeyCode::Tab | KeyCode::Down => {
            let new_field = (field + 1) % total;
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &extra_fields);
            state.mode = InputMode::Adding { name, desc, extra_fields, field: new_field, cursor: new_cursor };
        }
        KeyCode::Up => {
            let new_field = (field + total - 1) % total;
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &extra_fields);
            state.mode = InputMode::Adding { name, desc, extra_fields, field: new_field, cursor: new_cursor };
        }
        KeyCode::Left if !is_on_add_button => {
            let new_cursor = cursor.saturating_sub(1);
            state.mode = InputMode::Adding { name, desc, extra_fields, field, cursor: new_cursor };
        }
        KeyCode::Right if !is_on_add_button => {
            let new_cursor = (cursor + 1).min(active_len);
            state.mode = InputMode::Adding { name, desc, extra_fields, field, cursor: new_cursor };
        }
        KeyCode::Home if !is_on_add_button => {
            state.mode = InputMode::Adding { name, desc, extra_fields, field, cursor: 0 };
        }
        KeyCode::End if !is_on_add_button => {
            state.mode = InputMode::Adding { name, desc, extra_fields, field, cursor: active_len };
        }
        KeyCode::Backspace if !is_on_add_button => {
            if cursor == 0 {
                state.mode = InputMode::Adding { name, desc, extra_fields, field, cursor };
                return;
            }
            let (new_name, new_desc, new_extra_fields, new_cursor) = if field == 0 {
                let mut n = name;
                n.remove(cursor - 1);
                (n, desc, extra_fields, cursor - 1)
            } else if field == 1 {
                let mut d = desc;
                d.remove(cursor - 1);
                (name, d, extra_fields, cursor - 1)
            } else if is_on_extra {
                let mut ef = extra_fields;
                let idx = field - 2;
                ef[idx].value.remove(cursor - 1);
                (name, desc, ef, cursor - 1)
            } else {
                (name, desc, extra_fields, cursor)
            };
            state.mode = InputMode::Adding { name: new_name, desc: new_desc, extra_fields: new_extra_fields, field, cursor: new_cursor };
        }
        KeyCode::Char(c) if !is_on_add_button => {
            let (new_name, new_desc, new_extra_fields, new_cursor) = if field == 0 {
                let mut n = name;
                n.insert(cursor, c);
                (n, desc, extra_fields, cursor + 1)
            } else if field == 1 {
                let mut d = desc;
                d.insert(cursor, c);
                (name, d, extra_fields, cursor + 1)
            } else if is_on_extra {
                let mut ef = extra_fields;
                let idx = field - 2;
                ef[idx].value.insert(cursor, c);
                (name, desc, ef, cursor + 1)
            } else {
                (name, desc, extra_fields, cursor)
            };
            state.mode = InputMode::Adding { name: new_name, desc: new_desc, extra_fields: new_extra_fields, field, cursor: new_cursor };
        }
        _ => {
            state.mode = InputMode::Adding { name, desc, extra_fields, field, cursor };
        }
    }
}

fn handle_editing(key: event::KeyEvent, state: &mut TreeState) {
    let (node_id, name, desc, extra_fields, field, cursor) = match &state.mode {
        InputMode::Editing { node_id, name, desc, extra_fields, field, cursor } => {
            (*node_id, name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor)
        }
        _ => return,
    };

    let total = total_fields(&extra_fields);
    let is_on_extra = extra_field_index(field).map_or(false, |i| i < extra_fields.len());
    let is_on_add_button = field == 2 + extra_fields.len();
    let active_len = active_len_for_field(&name, &desc, &extra_fields, field);

    match key.code {
        KeyCode::Delete if is_on_extra => {
            state.mode = InputMode::ConfirmDeleteField {
                node_id: Some(node_id),
                name, desc, extra_fields, field, cursor,
                target_index: field - 2,
            };
        }
        KeyCode::Enter if is_on_add_button => {
            state.mode = InputMode::SelectFieldType {
                node_id: Some(node_id),
                name, desc, extra_fields, field, cursor,
                selected_type: 0,
            };
        }
        KeyCode::Enter => {
            state.update_node(node_id, name, desc, extra_fields);
            state.mode = InputMode::Detail { selected: 0, alert: None };
        }
        KeyCode::Esc => {
            if editing_has_changes(state, node_id, &name, &desc, &extra_fields) {
                state.mode = InputMode::ConfirmDiscardChanges {
                    node_id: Some(node_id), name, desc, extra_fields, field, cursor,
                };
            } else {
                state.mode = InputMode::Detail { selected: 0, alert: None };
            }
        }
        KeyCode::Tab | KeyCode::Down => {
            let new_field = (field + 1) % total;
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &extra_fields);
            state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field: new_field, cursor: new_cursor };
        }
        KeyCode::Up => {
            let new_field = (field + total - 1) % total;
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &extra_fields);
            state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field: new_field, cursor: new_cursor };
        }
        KeyCode::Left if !is_on_add_button => {
            let new_cursor = cursor.saturating_sub(1);
            state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field, cursor: new_cursor };
        }
        KeyCode::Right if !is_on_add_button => {
            let new_cursor = (cursor + 1).min(active_len);
            state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field, cursor: new_cursor };
        }
        KeyCode::Home if !is_on_add_button => {
            state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field, cursor: 0 };
        }
        KeyCode::End if !is_on_add_button => {
            state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field, cursor: active_len };
        }
        KeyCode::Backspace if !is_on_add_button => {
            if cursor == 0 {
                state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field, cursor };
                return;
            }
            let (new_name, new_desc, new_extra_fields, new_cursor) = if field == 0 {
                let mut n = name;
                n.remove(cursor - 1);
                (n, desc, extra_fields, cursor - 1)
            } else if field == 1 {
                let mut d = desc;
                d.remove(cursor - 1);
                (name, d, extra_fields, cursor - 1)
            } else if is_on_extra {
                let mut ef = extra_fields;
                let idx = field - 2;
                ef[idx].value.remove(cursor - 1);
                (name, desc, ef, cursor - 1)
            } else {
                (name, desc, extra_fields, cursor)
            };
            state.mode = InputMode::Editing { node_id, name: new_name, desc: new_desc, extra_fields: new_extra_fields, field, cursor: new_cursor };
        }
        KeyCode::Char(c) if !is_on_add_button => {
            let (new_name, new_desc, new_extra_fields, new_cursor) = if field == 0 {
                let mut n = name;
                n.insert(cursor, c);
                (n, desc, extra_fields, cursor + 1)
            } else if field == 1 {
                let mut d = desc;
                d.insert(cursor, c);
                (name, d, extra_fields, cursor + 1)
            } else if is_on_extra {
                let mut ef = extra_fields;
                let idx = field - 2;
                ef[idx].value.insert(cursor, c);
                (name, desc, ef, cursor + 1)
            } else {
                (name, desc, extra_fields, cursor)
            };
            state.mode = InputMode::Editing { node_id, name: new_name, desc: new_desc, extra_fields: new_extra_fields, field, cursor: new_cursor };
        }
        _ => {
            state.mode = InputMode::Editing { node_id, name, desc, extra_fields, field, cursor };
        }
    }
}

fn handle_select_field_type(key: event::KeyEvent, state: &mut TreeState) {
    let (node_id, name, desc, extra_fields, field, cursor, selected_type) = match &state.mode {
        InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type } => {
            (*node_id, name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor, *selected_type)
        }
        _ => return,
    };

    let is_adding = node_id.is_none();

    match key.code {
        KeyCode::Esc => {
            state.mode = build_editing_mode(node_id, name, desc, extra_fields, field, cursor, is_adding);
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let new_sel = (selected_type + 1) % 3;
            state.mode = InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type: new_sel };
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let new_sel = (selected_type + 2) % 3;
            state.mode = InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type: new_sel };
        }
        KeyCode::Char('1') => {
            state.mode = InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type: 0 };
        }
        KeyCode::Char('2') => {
            state.mode = InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type: 1 };
        }
        KeyCode::Char('3') => {
            state.mode = InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type: 2 };
        }
        KeyCode::Enter => {
            match selected_type {
                0 => {
                    let mut ef = extra_fields;
                    ef.push(ExtraField { field_type: FieldType::Login, value: String::new() });
                    let new_field = 2 + ef.len() - 1;
                    state.mode = build_editing_mode(node_id, name, desc, ef, new_field, 0, is_adding);
                }
                1 => {
                    let mut ef = extra_fields;
                    ef.push(ExtraField { field_type: FieldType::Password, value: String::new() });
                    let new_field = 2 + ef.len() - 1;
                    state.mode = build_editing_mode(node_id, name, desc, ef, new_field, 0, is_adding);
                }
                _ => {
                    state.mode = InputMode::CustomFieldName { node_id, name, desc, extra_fields, field, cursor, input: String::new() };
                }
            }
        }
        _ => {
            state.mode = InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type };
        }
    }
}

fn handle_custom_field_name(key: event::KeyEvent, state: &mut TreeState) {
    let (node_id, name, desc, extra_fields, field, cursor, input) = match &state.mode {
        InputMode::CustomFieldName { node_id, name, desc, extra_fields, field, cursor, input } => {
            (*node_id, name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor, input.clone())
        }
        _ => return,
    };

    let is_adding = node_id.is_none();

    match key.code {
        KeyCode::Esc => {
            state.mode = InputMode::SelectFieldType { node_id, name, desc, extra_fields, field, cursor, selected_type: 2 };
        }
        KeyCode::Enter => {
            let label = if input.is_empty() { "Custom".to_string() } else { input };
            let mut ef = extra_fields;
            ef.push(ExtraField { field_type: FieldType::Custom(label), value: String::new() });
            let new_field = 2 + ef.len() - 1;
            state.mode = build_editing_mode(node_id, name, desc, ef, new_field, 0, is_adding);
        }
        KeyCode::Backspace => {
            let mut new_input = input;
            new_input.pop();
            state.mode = InputMode::CustomFieldName { node_id, name, desc, extra_fields, field, cursor, input: new_input };
        }
        KeyCode::Char(c) => {
            let mut new_input = input;
            new_input.push(c);
            state.mode = InputMode::CustomFieldName { node_id, name, desc, extra_fields, field, cursor, input: new_input };
        }
        _ => {
            state.mode = InputMode::CustomFieldName { node_id, name, desc, extra_fields, field, cursor, input };
        }
    }
}

fn handle_confirm_delete(key: event::KeyEvent, state: &mut TreeState) {
    let (node_id, name, desc, extra_fields, field, cursor, target_index) = match &state.mode {
        InputMode::ConfirmDeleteField { node_id, name, desc, extra_fields, field, cursor, target_index } => {
            (*node_id, name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor, *target_index)
        }
        _ => return,
    };

    let is_adding = node_id.is_none();

    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let mut ef = extra_fields;
            if target_index < ef.len() {
                ef.remove(target_index);
            }
            let total = total_fields(&ef);
            let new_field = field.min(total.saturating_sub(1));
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &ef);
            state.mode = build_editing_mode(node_id, name, desc, ef, new_field, new_cursor, is_adding);
        }
        _ => {
            state.mode = build_editing_mode(node_id, name, desc, extra_fields, field, cursor, is_adding);
        }
    }
}

fn editing_has_changes(
    state: &TreeState,
    node_id: uuid::Uuid,
    name: &str,
    desc: &str,
    extra_fields: &[ExtraField],
) -> bool {
    let original = state.root.find(node_id);
    match original {
        None => false,
        Some(n) => {
            if name != n.name || desc != n.description {
                return true;
            }
            if extra_fields.len() != n.extra_fields.len() {
                return true;
            }
            for (a, b) in extra_fields.iter().zip(n.extra_fields.iter()) {
                if a.value != b.value {
                    return true;
                }
                match (&a.field_type, &b.field_type) {
                    (FieldType::Login, FieldType::Login)
                    | (FieldType::Password, FieldType::Password) => {}
                    (FieldType::Custom(an), FieldType::Custom(bn)) if an == bn => {}
                    _ => return true,
                }
            }
            false
        }
    }
}

fn adding_has_changes(name: &str, desc: &str, extra_fields: &[ExtraField]) -> bool {
    !name.is_empty() || !desc.is_empty() || !extra_fields.is_empty()
}

fn handle_confirm_discard(key: event::KeyEvent, state: &mut TreeState) {
    let (node_id, name, desc, extra_fields, field, cursor) = match &state.mode {
        InputMode::ConfirmDiscardChanges { node_id, name, desc, extra_fields, field, cursor } => {
            (*node_id, name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor)
        }
        _ => return,
    };

    match key.code {
        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
            if node_id.is_some() {
                state.mode = InputMode::Detail { selected: 0, alert: None };
            } else {
                state.mode = InputMode::Normal;
            }
        }
        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
            if let Some(nid) = node_id {
                state.mode = InputMode::Editing { node_id: nid, name, desc, extra_fields, field, cursor };
            } else {
                state.mode = InputMode::Adding { name, desc, extra_fields, field, cursor };
            }
        }
        _ => {}
    }
}

fn copy_to_clipboard(text: &str) {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[&["wl-copy"], &["xclip", "-selection", "clipboard"], &["xsel", "-b"]]
    } else {
        return;
    };
    for args in cmds {
        if let Ok(mut child) = std::process::Command::new(args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            break;
        }
    }
}

fn handle_detail(key: event::KeyEvent, state: &mut TreeState) {
    let (selected, alert) = match &state.mode {
        InputMode::Detail { selected, alert } => (*selected, alert.clone()),
        _ => return,
    };

    if alert.is_some() {
        state.mode = InputMode::Detail { selected, alert: None };
        return;
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char(' ') => {
            state.mode = InputMode::Normal;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            let node = state.current_node();
            let max = node.map_or(0, |n| n.extra_fields.len());
            let new_sel = (selected + 1).min(max);
            state.mode = InputMode::Detail { selected: new_sel, alert: None };
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let new_sel = selected.saturating_sub(1);
            state.mode = InputMode::Detail { selected: new_sel, alert: None };
        }
        KeyCode::Enter => {
            let text = state.current_node().and_then(|node| {
                if selected == 0 {
                    Some(node.description.clone())
                } else {
                    let idx = selected - 1;
                    node.extra_fields.get(idx).map(|ef| ef.value.clone())
                }
            });
            if let Some(ref t) = text {
                if !t.is_empty() {
                    copy_to_clipboard(t);
                }
            }
            state.mode = InputMode::Detail { selected, alert: Some("Copied!".to_string()) };
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if let Some(node) = state.current_node() {
                let cursor = node.name.len();
                state.mode = InputMode::Editing {
                    node_id: node.id,
                    name: node.name,
                    desc: node.description,
                    extra_fields: node.extra_fields,
                    field: 0,
                    cursor,
                };
            }
        }
        _ => {}
    }
}

fn ui(frame: &mut Frame, state: &mut TreeState) {
    let area = frame.area();

    let layout = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    TreeWidget::render(frame, layout[0], state);
    render_status_bar(frame, layout[1], state);
    render_help_bar(frame, layout[2], state);

    match &state.mode {
        InputMode::Search => render_search_overlay(frame, area, state),
        InputMode::Adding { .. } => render_add_overlay(frame, area, state),
        InputMode::Editing { .. } => render_edit_overlay(frame, area, state),
        InputMode::SelectFieldType { .. } => render_select_field_type_overlay(frame, area, state),
        InputMode::CustomFieldName { .. } => render_custom_field_name_overlay(frame, area, state),
        InputMode::ConfirmDeleteField { .. } => render_confirm_delete_overlay(frame, area, state),
        InputMode::ConfirmDiscardChanges { .. } => render_confirm_discard_overlay(frame, area, state),
        InputMode::Detail { .. } => {
            render_detail_overlay(frame, area, state);
            if matches!(state.mode, InputMode::Detail { ref alert, .. } if alert.is_some()) {
                render_copy_alert_overlay(frame, area);
            }
        }
        _ => {}
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (mode_str, mode_style) = match &state.mode {
        InputMode::Normal => ("NORMAL", Style::default().bg(Color::DarkGray).fg(Color::White)),
        InputMode::Search => ("SEARCH", Style::default().bg(Color::Blue).fg(Color::White)),
        InputMode::AwaitingDd => ("AWAIT DD", Style::default().bg(Color::Red).fg(Color::White)),
        InputMode::Detail { .. } => ("DETAIL", Style::default().bg(Color::Magenta).fg(Color::White)),
        InputMode::Editing { .. } => ("EDIT", Style::default().bg(Color::Yellow).fg(Color::Black)),
        InputMode::Adding { .. } => ("ADD", Style::default().bg(Color::Green).fg(Color::Black)),
        InputMode::SelectFieldType { .. } => ("FIELD TYPE", Style::default().bg(Color::Blue).fg(Color::White)),
        InputMode::CustomFieldName { .. } => ("CUSTOM NAME", Style::default().bg(Color::Blue).fg(Color::White)),
        InputMode::ConfirmDeleteField { .. } => ("CONFIRM", Style::default().bg(Color::Red).fg(Color::White)),
        InputMode::ConfirmDiscardChanges { .. } => ("DISCARD?", Style::default().bg(Color::Yellow).fg(Color::Red)),
    };

    let name = state.current_name().unwrap_or_default();

    let text = Text::from(Line::from(vec![
        Span::styled(format!(" {} ", mode_str), mode_style.add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(
            format!(
                "{} items{} | {}",
                state.num_visible(),
                if state.search_active {
                    format!(" ({} matches)", state.num_matched())
                } else {
                    String::new()
                },
                if state.search_active {
                    format!("/{}", state.search_query)
                } else {
                    name
                }
            ),
            Style::default().fg(Color::Gray),
        ),
    ]));

    frame.render_widget(Paragraph::new(text), area);
}

fn render_help_bar(frame: &mut Frame, area: Rect, state: &TreeState) {
    let help = match &state.mode {
        InputMode::Normal => {
            "j/k:nav  h/l:collapse  Enter/l:open  i:detail  dd:cut  p:paste  P:paste_child  o:add  /:search  q:quit"
        }
        InputMode::Search => "Type to search  Enter:done  Esc:cancel",
        InputMode::AwaitingDd => "Press d again to cut",
        InputMode::Detail { ref alert, .. } if alert.is_some() => "Press any key to continue",
        InputMode::Detail { .. } => "j/k:navigate  Enter:copy  i:edit  Esc:close",
        InputMode::Editing { extra_fields, field, .. } => {
            let is_on_extra = extra_field_index(*field).map_or(false, |i| i < extra_fields.len());
            if is_on_extra {
                "Tab:switch  Del:delete  Enter:save  Esc:cancel"
            } else if *field == 2 + extra_fields.len() {
                "Tab:switch  Enter:add field  Esc:cancel"
            } else {
                "Tab:switch  Enter:save  Esc:cancel"
            }
        }
        InputMode::Adding { extra_fields, field, .. } => {
            let is_on_extra = extra_field_index(*field).map_or(false, |i| i < extra_fields.len());
            if is_on_extra {
                "Tab:switch  Del:delete  Enter:save  Esc:cancel"
            } else if *field == 2 + extra_fields.len() {
                "Tab:switch  Enter:add field  Esc:cancel"
            } else {
                "Tab:switch  Enter:save  Esc:cancel"
            }
        }
        InputMode::SelectFieldType { .. } => "j/k:nav  1/2/3:select  Enter:choose  Esc:cancel",
        InputMode::CustomFieldName { .. } => "Type name  Enter:done  Esc:cancel",
        InputMode::ConfirmDeleteField { .. } => "y:delete  n/Esc:cancel",
        InputMode::ConfirmDiscardChanges { .. } => "Enter/y:discard  n/Esc:continue editing",
    };

    let text = Text::from(Line::from(vec![Span::styled(
        help,
        Style::default().fg(Color::DarkGray),
    )]));

    frame.render_widget(Paragraph::new(text), area);
}

fn render_search_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 1,
        width: area.width / 2,
        height: 3,
    };

    let block = Block::default()
        .title(" Search ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let text = Text::from(Line::from(vec![Span::styled(
        format!("/{}", state.search_query),
        Style::default().fg(Color::White),
    )]));

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(text), inner);
}

fn cursor_field_spans(label: &str, text: &str, cursor: usize, is_active: bool) -> Vec<Span<'static>> {
    let label_style = Style::default().fg(Color::DarkGray);
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default().fg(Color::White);
    let inverted_style = Style::default().bg(Color::White).fg(Color::Black);

    if !is_active {
        return vec![
            Span::styled(label.to_string(), label_style),
            Span::styled(text.to_string(), dim_style),
        ];
    }

    let pos = cursor.min(text.len());
    let (before, rest) = text.split_at(pos);
    let ch = rest.chars().next();
    let after = &rest[ch.map(|c| c.len_utf8()).unwrap_or(0)..];

    let mut spans = vec![Span::styled(label.to_string(), label_style)];
    if !before.is_empty() {
        spans.push(Span::styled(before.to_string(), normal_style));
    }
    match ch {
        Some(c) => {
            let mut s = String::new();
            s.push(c);
            spans.push(Span::styled(s, inverted_style));
        }
        None => {
            spans.push(Span::styled(" ".to_string(), inverted_style));
        }
    }
    if !after.is_empty() {
        spans.push(Span::styled(after.to_string(), normal_style));
    }
    spans
}

fn make_overlay_area(area: Rect, extra_fields_len: usize, base_content: u16) -> Rect {
    let n = extra_fields_len as u16;
    let content_height = base_content.saturating_add(n);
    let overlay_height = (content_height + 2).min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 5,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width * 3 / 5,
        height: overlay_height,
    }
}

fn render_extra_field_line(ef: &ExtraField, ef_index: usize, field: usize, cursor: usize, is_inactive_display: bool) -> Line<'static> {
    let is_active = field == ef_index + 2;
    let display_val = if is_active && !is_inactive_display {
        ef.value.clone()
    } else {
        ef.field_type.display_value(&ef.value)
    };
    let prefix = format!("{}: ", ef.field_type.label());
    Line::from(cursor_field_spans(&prefix, &display_val, cursor, is_active))
}

fn render_edit_overlay_common(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    desc: &str,
    extra_fields: &[ExtraField],
    field: usize,
    cursor: usize,
    title: &str,
) {
    let overlay_area = make_overlay_area(area, extra_fields.len(), 5);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(cursor_field_spans("Name: ", name, cursor, field == 0)));
    lines.push(Line::from(cursor_field_spans("Desc: ", desc, cursor, field == 1)));

    for (i, ef) in extra_fields.iter().enumerate() {
        lines.push(render_extra_field_line(ef, i, field, cursor, false));
    }

    let is_add_active = field == 2 + extra_fields.len();
    let add_style = if is_add_active {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };
    lines.push(Line::from(Span::styled("+ Add Field", add_style)));
    lines.push(Line::from(""));

    let hint = if field >= 2 && field < 2 + extra_fields.len() {
        "Tab:switch  Del:delete  Enter:save  Esc:cancel"
    } else if field == 2 + extra_fields.len() {
        "Tab:switch  Enter:add field  Esc:cancel"
    } else {
        "Tab:switch  Enter:save  Esc:cancel"
    };
    lines.push(Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_add_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (name, desc, extra_fields, field, cursor) = match &state.mode {
        InputMode::Adding { name, desc, extra_fields, field, cursor } => {
            (name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor)
        }
        _ => return,
    };

    render_edit_overlay_common(frame, area, &name, &desc, &extra_fields, field, cursor, "Add Node");
}

fn render_edit_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (name, desc, extra_fields, field, cursor) = match &state.mode {
        InputMode::Editing { name, desc, extra_fields, field, cursor, .. } => {
            (name.clone(), desc.clone(), extra_fields.clone(), *field, *cursor)
        }
        _ => return,
    };

    render_edit_overlay_common(frame, area, &name, &desc, &extra_fields, field, cursor, "Edit Node");
}

fn render_select_field_type_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let selected_type = match &state.mode {
        InputMode::SelectFieldType { selected_type, .. } => *selected_type,
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 4,
        width: area.width / 2,
        height: 9,
    };

    let block = Block::default()
        .title(" Select Field Type ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let items = ["1. Login", "2. Password", "3. Custom Field"];
    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let style = if i == selected_type {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if i == selected_type { "▸ " } else { "  " };
        lines.push(Line::from(Span::styled(format!("{}{}", prefix, item), style)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k:nav  1/2/3:select  Enter:choose  Esc:cancel",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_custom_field_name_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let input = match &state.mode {
        InputMode::CustomFieldName { input, .. } => input.clone(),
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 2,
        width: area.width / 2,
        height: 5,
    };

    let block = Block::default()
        .title(" Custom Field Name ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(cursor_field_spans("Name: ", &input, input.len(), true)),
        Line::from(""),
        Line::from(Span::styled(
            "Enter:done  Esc:cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_confirm_delete_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let label = match &state.mode {
        InputMode::ConfirmDeleteField { ref extra_fields, target_index, .. } => {
            extra_fields.get(*target_index).map(|ef| ef.field_type.label()).unwrap_or_default()
        }
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 2,
        width: area.width / 2,
        height: 5,
    };

    let block = Block::default()
        .title(" Confirm Delete ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Red));

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(Span::styled(
            format!("Delete \"{}\" field? (y/N)", label),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y:delete  n/Esc:cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_copy_alert_overlay(frame: &mut Frame, area: Rect) {
    let overlay_area = Rect {
        x: area.width / 3,
        y: area.height / 2 - 2,
        width: area.width / 3,
        height: 5,
    };

    let block = Block::default()
        .title(" Copied ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Green));

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "✓ Copied!",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_confirm_discard_overlay(frame: &mut Frame, area: Rect, _state: &TreeState) {
    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 2,
        width: area.width / 2,
        height: 5,
    };

    let block = Block::default()
        .title(" Unsaved Changes ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Yellow));

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Discard changes?",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Enter/y:discard  n/Esc:continue editing",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_detail_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let node = match state.current_node() {
        Some(n) => n,
        None => return,
    };

    let selected = match &state.mode {
        InputMode::Detail { selected, .. } => *selected,
        _ => 0,
    };

    let desc = if node.description.is_empty() {
        "No description.".to_string()
    } else {
        node.description.clone()
    };

    let overlay_area = make_overlay_area(area, node.extra_fields.len(), 4);

    let block = Block::default()
        .title(format!(" {} ", node.name))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    let desc_style = if selected == 0 {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };
    let desc_prefix = if selected == 0 { "▸ " } else { "  " };
    lines.push(Line::from(Span::styled(
        format!("{}{}", desc_prefix, desc),
        desc_style,
    )));

    for (i, ef) in node.extra_fields.iter().enumerate() {
        let ef_index = i + 1;
        let display_val = ef.field_type.display_value(&ef.value);
        let is_sel = selected == ef_index;
        let style = if is_sel {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if is_sel { "▸ " } else { "  " };
        lines.push(Line::from(Span::styled(
            format!("{}{}: {}", prefix, ef.field_type.label(), display_val),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k:navigate  Enter:copy  i:edit  Esc:close",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}
