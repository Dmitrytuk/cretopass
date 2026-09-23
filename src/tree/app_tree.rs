use std::borrow::Cow;
use std::io::{self, Write};

use crate::cursor::{
    backspace_at, cursor_field_spans, cursor_left, cursor_right, delete_at, insert_char_at,
    selected_span,
};
use crate::file_handling::save_tree;
use crate::theming::Action;
use crate::theming::{texts, ElementStyle};
use crate::tree::node::{ExtraField, FieldType};
use crate::tree::widget::{InputMode, TreeState, TreeWidget};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use ratatui::{backend::Backend, Terminal};

pub fn run_app<B: Backend>(terminal: &mut Terminal<B>, state: &mut TreeState) -> io::Result<()> {
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
                InputMode::Adding { .. } | InputMode::Editing { .. } => handle_form(key, state),
                InputMode::SelectFieldType { .. } => handle_select_field_type(key, state),
                InputMode::CustomFieldName { .. } => handle_custom_field_name(key, state),
                InputMode::ConfirmDeleteField { .. } => handle_confirm_delete(key, state),
                InputMode::ConfirmDiscardChanges { .. } => handle_confirm_discard(key, state),
                InputMode::Detail { .. } => handle_detail(key, state),
                InputMode::ExportNodeSelect { .. } => handle_export_node_select(key, state),
                InputMode::ExportNodeFormat { .. } => handle_export_node_format(key, state),
                InputMode::Alert { .. } => handle_alert(key, state),
                InputMode::Help => handle_help(key, state),
            }

            if state.dirty {
                state.status = Some(texts::STATUS_SAVING);
                terminal.draw(|f| ui(f, state))?;
                if let Some(ref p) = state.active_file_path {
                    state.dirty = false;
                    if let Err(e) = save_tree(&state.root, p, &state.active_file_passphrase) {
                        eprintln!("Save failed: {e}");
                    }
                }
                state.status = None;
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
    match Action::from_normal(&key) {
        Action::Quit => {
            state.quit = true;
        }
        Action::ReturnToMenu => {
            state.return_to_menu = true;
            state.quit = true;
        }
        Action::NavDown => {
            state.select_next();
        }
        Action::NavUp => {
            state.select_prev();
        }
        Action::NavRight => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            } else {
                state.expand_collapse(true);
            }
        }
        Action::NavLeft => {
            state.expand_collapse(false);
        }
        Action::Confirm => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            } else {
                state.toggle_expand();
            }
        }
        Action::ToggleExpand => {
            state.toggle_expand();
        }
        Action::Cut => {
            state.mode = InputMode::AwaitingDd;
        }
        Action::PasteSibling => {
            state.paste_sibling();
        }
        Action::PasteChild => {
            state.paste_child();
        }
        Action::OpenDetail => {
            state.mode = InputMode::Detail {
                selected: 0,
                alert: None,
            };
        }
        Action::AddNode => {
            state.adding_as_sibling = false;
            state.mode = InputMode::Adding {
                name: String::new(),
                desc: String::new(),
                extra_fields: Vec::new(),
                field: 0,
                cursor: 0,
            };
        }
        Action::AddSiblingNode => {
            let is_root = state.selected_node_id() == Some(state.root.id);
            state.adding_as_sibling = !is_root;
            state.mode = InputMode::Adding {
                name: String::new(),
                desc: String::new(),
                extra_fields: Vec::new(),
                field: 0,
                cursor: 0,
            };
        }
        Action::ToggleSearch => {
            state.mode = InputMode::Search;
        }
        Action::ExportNode => {
            if let Some(node) = state.current_node_ref() {
                state.mode = InputMode::ExportNodeSelect {
                    node: node.clone(),
                    selected: 0,
                };
            }
        }
        Action::Cancel => {
            state.clear_search();
        }
        Action::OpenHelp => {
            state.mode = InputMode::Help;
        }
        _ => {}
    }
}

fn handle_search(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_search(&key) {
        Action::ConfirmSearch => {
            state.mode = InputMode::Normal;
        }
        Action::ClearSearch => {
            state.clear_search();
            state.mode = InputMode::Normal;
        }
        Action::Backspace => {
            let mut q = state.search_query.clone();
            q.pop();
            state.set_search(q);
        }
        Action::InsertChar(c) => {
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

fn active_len_for_field(
    name: &str,
    desc: &str,
    extra_fields: &[ExtraField],
    field: usize,
) -> usize {
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

fn active_text_for_field<'a>(
    name: &'a str,
    desc: &'a str,
    extra_fields: &'a [ExtraField],
    field: usize,
) -> Option<&'a str> {
    match field {
        0 => Some(name),
        1 => Some(desc),
        f => f
            .checked_sub(2)
            .and_then(|ef_idx| extra_fields.get(ef_idx).map(|ef| ef.value.as_str())),
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
        InputMode::Adding {
            name,
            desc,
            extra_fields,
            field,
            cursor,
        }
    } else if let Some(nid) = node_id {
        InputMode::Editing {
            node_id: nid,
            name,
            desc,
            extra_fields,
            field,
            cursor,
        }
    } else {
        InputMode::Adding {
            name,
            desc,
            extra_fields,
            field,
            cursor,
        }
    }
}

fn handle_form(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (is_adding, node_id, name, desc, extra_fields, field, cursor) = match mode {
        InputMode::Adding {
            name,
            desc,
            extra_fields,
            field,
            cursor,
        } => (true, None, name, desc, extra_fields, field, cursor),
        InputMode::Editing {
            node_id,
            name,
            desc,
            extra_fields,
            field,
            cursor,
        } => (
            false,
            Some(node_id),
            name,
            desc,
            extra_fields,
            field,
            cursor,
        ),
        other => {
            state.mode = other;
            return;
        }
    };

    let build_mode = |n: String, d: String, ef: Vec<ExtraField>, f: usize, cur: usize| {
        if let Some(id) = node_id {
            InputMode::Editing {
                node_id: id,
                name: n,
                desc: d,
                extra_fields: ef,
                field: f,
                cursor: cur,
            }
        } else {
            InputMode::Adding {
                name: n,
                desc: d,
                extra_fields: ef,
                field: f,
                cursor: cur,
            }
        }
    };

    let total = total_fields(&extra_fields);
    let is_on_extra = extra_field_index(field).is_some_and(|i| i < extra_fields.len());
    let is_on_add_button = field == 2 + extra_fields.len();
    let active_len = active_len_for_field(&name, &desc, &extra_fields, field);

    match Action::from_form(&key) {
        Action::DeleteChar if is_on_extra => {
            state.mode = InputMode::ConfirmDeleteField {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                target_index: field - 2,
            };
        }
        Action::Confirm if is_on_add_button => {
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type: 0,
            };
        }
        Action::Confirm => {
            if is_adding {
                if state.adding_as_sibling {
                    state.add_sibling_to_selected(name, desc, extra_fields);
                } else {
                    state.add_child_to_selected(name, desc, extra_fields);
                }
                state.mode = InputMode::Normal;
            } else {
                state.update_node(node_id.unwrap(), name, desc, extra_fields);
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            }
        }
        Action::Cancel => {
            let has_changes = if is_adding {
                adding_has_changes(&name, &desc, &extra_fields)
            } else {
                editing_has_changes(state, node_id.unwrap(), &name, &desc, &extra_fields)
            };
            if has_changes {
                state.mode = InputMode::ConfirmDiscardChanges {
                    node_id,
                    name,
                    desc,
                    extra_fields,
                    field,
                    cursor,
                };
            } else if is_adding {
                state.mode = InputMode::Normal;
            } else {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            }
        }
        Action::NextField => {
            let new_field = (field + 1) % total;
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &extra_fields);
            state.mode = build_mode(name, desc, extra_fields, new_field, new_cursor);
        }
        Action::PrevField => {
            let new_field = (field + total - 1) % total;
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &extra_fields);
            state.mode = build_mode(name, desc, extra_fields, new_field, new_cursor);
        }
        Action::CursorLeft if !is_on_add_button => {
            let new_cursor = active_text_for_field(&name, &desc, &extra_fields, field)
                .map_or(cursor, |text| cursor_left(text, cursor));
            state.mode = build_mode(name, desc, extra_fields, field, new_cursor);
        }
        Action::CursorRight if !is_on_add_button => {
            let new_cursor = active_text_for_field(&name, &desc, &extra_fields, field)
                .map_or(cursor, |text| cursor_right(text, cursor));
            state.mode = build_mode(name, desc, extra_fields, field, new_cursor);
        }
        Action::CursorHome if !is_on_add_button => {
            state.mode = build_mode(name, desc, extra_fields, field, 0);
        }
        Action::CursorEnd if !is_on_add_button => {
            state.mode = build_mode(name, desc, extra_fields, field, active_len);
        }
        Action::Backspace if !is_on_add_button => {
            if cursor == 0 {
                state.mode = build_mode(name, desc, extra_fields, field, cursor);
                return;
            }
            let (new_name, new_desc, new_extra_fields, new_cursor) = if field == 0 {
                let mut n = name;
                let nc = backspace_at(&mut n, cursor);
                (n, desc, extra_fields, nc)
            } else if field == 1 {
                let mut d = desc;
                let nc = backspace_at(&mut d, cursor);
                (name, d, extra_fields, nc)
            } else if is_on_extra {
                let mut ef = extra_fields;
                let idx = field - 2;
                let nc = backspace_at(&mut ef[idx].value, cursor);
                (name, desc, ef, nc)
            } else {
                (name, desc, extra_fields, cursor)
            };
            state.mode = build_mode(new_name, new_desc, new_extra_fields, field, new_cursor);
        }
        Action::DeleteChar if !is_on_add_button => {
            if cursor >= active_len {
                state.mode = build_mode(name, desc, extra_fields, field, cursor);
                return;
            }
            let (new_name, new_desc, new_extra_fields, new_cursor) = if field == 0 {
                let mut n = name;
                delete_at(&mut n, cursor);
                (n, desc, extra_fields, cursor)
            } else if field == 1 {
                let mut d = desc;
                delete_at(&mut d, cursor);
                (name, d, extra_fields, cursor)
            } else if is_on_extra {
                let mut ef = extra_fields;
                let idx = field - 2;
                delete_at(&mut ef[idx].value, cursor);
                (name, desc, ef, cursor)
            } else {
                (name, desc, extra_fields, cursor)
            };
            state.mode = build_mode(new_name, new_desc, new_extra_fields, field, new_cursor);
        }
        Action::InsertChar(c) if !is_on_add_button => {
            let (new_name, new_desc, new_extra_fields, new_cursor) = if field == 0 {
                let mut n = name;
                let mut nc = cursor;
                insert_char_at(&mut n, &mut nc, c);
                (n, desc, extra_fields, nc)
            } else if field == 1 {
                let mut d = desc;
                let mut nc = cursor;
                insert_char_at(&mut d, &mut nc, c);
                (name, d, extra_fields, nc)
            } else if is_on_extra {
                let mut ef = extra_fields;
                let idx = field - 2;
                let mut nc = cursor;
                insert_char_at(&mut ef[idx].value, &mut nc, c);
                (name, desc, ef, nc)
            } else {
                (name, desc, extra_fields, cursor)
            };
            state.mode = build_mode(new_name, new_desc, new_extra_fields, field, new_cursor);
        }
        _ => {
            state.mode = build_mode(name, desc, extra_fields, field, cursor);
        }
    }
}

fn handle_select_field_type(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node_id, name, desc, extra_fields, field, cursor, selected_type) = match mode {
        InputMode::SelectFieldType {
            node_id,
            name,
            desc,
            extra_fields,
            field,
            cursor,
            selected_type,
        } => (
            node_id,
            name,
            desc,
            extra_fields,
            field,
            cursor,
            selected_type,
        ),
        other => {
            state.mode = other;
            return;
        }
    };

    let is_adding = node_id.is_none();

    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.mode =
                build_editing_mode(node_id, name, desc, extra_fields, field, cursor, is_adding);
        }
        Action::NavDown => {
            let new_sel = (selected_type + 1) % 3;
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type: new_sel,
            };
        }
        Action::NavUp => {
            let new_sel = (selected_type + 2) % 3;
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type: new_sel,
            };
        }
        Action::SelectType1 => {
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type: 0,
            };
        }
        Action::SelectType2 => {
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type: 1,
            };
        }
        Action::SelectType3 => {
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type: 2,
            };
        }
        Action::Confirm => match selected_type {
            0 => {
                let mut ef = extra_fields;
                ef.push(ExtraField {
                    field_type: FieldType::Login,
                    value: String::new(),
                });
                let new_field = 2 + ef.len() - 1;
                state.mode = build_editing_mode(node_id, name, desc, ef, new_field, 0, is_adding);
            }
            1 => {
                let mut ef = extra_fields;
                ef.push(ExtraField {
                    field_type: FieldType::Password,
                    value: String::new(),
                });
                let new_field = 2 + ef.len() - 1;
                state.mode = build_editing_mode(node_id, name, desc, ef, new_field, 0, is_adding);
            }
            _ => {
                state.mode = InputMode::CustomFieldName {
                    node_id,
                    name,
                    desc,
                    extra_fields,
                    field,
                    cursor,
                    input: String::new(),
                };
            }
        },
        _ => {
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type,
            };
        }
    }
}

fn handle_custom_field_name(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node_id, name, desc, extra_fields, field, cursor, input) = match mode {
        InputMode::CustomFieldName {
            node_id,
            name,
            desc,
            extra_fields,
            field,
            cursor,
            input,
        } => (node_id, name, desc, extra_fields, field, cursor, input),
        other => {
            state.mode = other;
            return;
        }
    };

    let is_adding = node_id.is_none();

    match Action::from_custom_field_name(&key) {
        Action::Cancel => {
            state.mode = InputMode::SelectFieldType {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                selected_type: 2,
            };
        }
        Action::Confirm => {
            let label = if input.is_empty() {
                texts::CUSTOM_FIELD_DEFAULT.to_string()
            } else {
                input
            };
            let mut ef = extra_fields;
            ef.push(ExtraField {
                field_type: FieldType::Custom(label),
                value: String::new(),
            });
            let new_field = 2 + ef.len() - 1;
            state.mode = build_editing_mode(node_id, name, desc, ef, new_field, 0, is_adding);
        }
        Action::Backspace => {
            let mut new_input = input;
            new_input.pop();
            state.mode = InputMode::CustomFieldName {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                input: new_input,
            };
        }
        Action::InsertChar(c) => {
            let mut new_input = input;
            new_input.push(c);
            state.mode = InputMode::CustomFieldName {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                input: new_input,
            };
        }
        _ => {
            state.mode = InputMode::CustomFieldName {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
                input,
            };
        }
    }
}

fn handle_confirm_delete(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node_id, name, desc, extra_fields, field, cursor, target_index) = match mode {
        InputMode::ConfirmDeleteField {
            node_id,
            name,
            desc,
            extra_fields,
            field,
            cursor,
            target_index,
        } => (
            node_id,
            name,
            desc,
            extra_fields,
            field,
            cursor,
            target_index,
        ),
        other => {
            state.mode = other;
            return;
        }
    };

    let is_adding = node_id.is_none();

    match Action::from_confirm(&key) {
        Action::Yes => {
            let mut ef = extra_fields;
            if target_index < ef.len() {
                ef.remove(target_index);
            }
            let total = total_fields(&ef);
            let new_field = field.min(total.saturating_sub(1));
            let new_cursor = cursor_on_new_field(new_field, &name, &desc, &ef);
            state.mode =
                build_editing_mode(node_id, name, desc, ef, new_field, new_cursor, is_adding);
        }
        _ => {
            state.mode =
                build_editing_mode(node_id, name, desc, extra_fields, field, cursor, is_adding);
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
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node_id, name, desc, extra_fields, field, cursor) = match mode {
        InputMode::ConfirmDiscardChanges {
            node_id,
            name,
            desc,
            extra_fields,
            field,
            cursor,
        } => (node_id, name, desc, extra_fields, field, cursor),
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_confirm(&key) {
        Action::Yes => {
            if node_id.is_some() {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            } else {
                state.mode = InputMode::Normal;
            }
        }
        Action::No => {
            if let Some(nid) = node_id {
                state.mode = InputMode::Editing {
                    node_id: nid,
                    name,
                    desc,
                    extra_fields,
                    field,
                    cursor,
                };
            } else {
                state.mode = InputMode::Adding {
                    name,
                    desc,
                    extra_fields,
                    field,
                    cursor,
                };
            }
        }
        _ => {
            state.mode = InputMode::ConfirmDiscardChanges {
                node_id,
                name,
                desc,
                extra_fields,
                field,
                cursor,
            };
        }
    }
}

fn copy_to_clipboard(text: &str) {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "-b"],
        ]
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
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (selected, alert) = match mode {
        InputMode::Detail { selected, alert } => (selected, alert),
        other => {
            state.mode = other;
            return;
        }
    };

    if alert.is_some() {
        state.mode = InputMode::Detail {
            selected,
            alert: None,
        };
        return;
    }

    match Action::from_detail(&key) {
        Action::Cancel => {
            state.mode = InputMode::Normal;
        }
        Action::NavDown => {
            let max = state.current_node_ref().map_or(0, |n| n.extra_fields.len());
            let new_sel = (selected + 1).min(max);
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::NavUp => {
            let new_sel = selected.saturating_sub(1);
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::CopyField => {
            let text = state.current_node_ref().and_then(|node| {
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
            state.mode = InputMode::Detail {
                selected,
                alert: Some(texts::COPY_CHECK.to_string()),
            };
        }
        Action::EditNode => {
            if let Some(node) = state.current_node_ref() {
                let cursor = node.name.len();
                state.mode = InputMode::Editing {
                    node_id: node.id,
                    name: node.name.clone(),
                    desc: node.description.clone(),
                    extra_fields: node.extra_fields.clone(),
                    field: 0,
                    cursor,
                };
            } else {
                state.mode = InputMode::Detail {
                    selected,
                    alert: None,
                };
            }
        }
        _ => {
            state.mode = InputMode::Detail {
                selected,
                alert: None,
            };
        }
    }
}

fn handle_export_node_select(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node, selected) = match mode {
        InputMode::ExportNodeSelect { node, selected } => (node, selected),
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.mode = InputMode::Normal;
        }
        Action::NavDown => {
            state.mode = InputMode::ExportNodeSelect {
                node,
                selected: (selected + 1) % 2,
            };
        }
        Action::NavUp => {
            state.mode = InputMode::ExportNodeSelect {
                node,
                selected: (selected + 1) % 2,
            };
        }
        Action::Confirm => {
            let include_children = selected == 1;
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: 0,
            };
        }
        Action::SelectType1 => {
            state.mode = InputMode::ExportNodeSelect { node, selected: 0 };
        }
        Action::SelectType2 => {
            state.mode = InputMode::ExportNodeSelect { node, selected: 1 };
        }
        _ => {
            state.mode = InputMode::ExportNodeSelect { node, selected };
        }
    }
}

fn handle_export_node_format(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node, include_children, selected_format) = match mode {
        InputMode::ExportNodeFormat {
            node,
            include_children,
            selected_format,
        } => (node, include_children, selected_format),
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.mode = InputMode::Normal;
        }
        Action::NavDown => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: (selected_format + 1) % 3,
            };
        }
        Action::NavUp => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: (selected_format + 2) % 3,
            };
        }
        Action::SelectType1 => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: 0,
            };
        }
        Action::SelectType2 => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: 1,
            };
        }
        Action::SelectType3 => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: 2,
            };
        }
        Action::Confirm => {
            use crate::file_handling::export::{export_tree, ExportFormat};

            let format = match selected_format {
                0 => ExportFormat::Json,
                1 => ExportFormat::Xml,
                _ => ExportFormat::Csv,
            };

            let node_name = node.name.clone();
            let export_node = if include_children {
                node
            } else {
                let mut n = node;
                n.children = Vec::new();
                n
            };

            let base_path = match &state.active_file_path {
                Some(p) => p.clone(),
                None => {
                    state.mode = InputMode::Alert {
                        message: texts::MSG_NO_ACTIVE_FILE.to_string(),
                    };
                    return;
                }
            };

            let path = std::path::Path::new(&base_path);
            let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("age");
            let node_name_clean: String = node_name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let export_path = parent.join(format!("{}_{}.{}", stem, node_name_clean, ext));
            let export_path_str = export_path.to_string_lossy().to_string();

            match export_tree(&export_node, &export_path_str, format) {
                Ok(out_path) => {
                    state.mode = InputMode::Alert {
                        message: format!("{}{}", texts::MSG_EXPORT_NODE_SUCCESS, out_path),
                    };
                }
                Err(e) => {
                    state.mode = InputMode::Alert {
                        message: format!("{e}"),
                    };
                }
            }
        }
        _ => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format,
            };
        }
    }
}

fn handle_alert(key: event::KeyEvent, state: &mut TreeState) {
    let _ = key;
    state.mode = InputMode::Normal;
}

fn handle_help(key: event::KeyEvent, state: &mut TreeState) {
    let _ = key;
    state.mode = InputMode::Normal;
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
        InputMode::Adding {
            name,
            desc,
            extra_fields,
            field,
            cursor,
        } => {
            render_edit_overlay(
                frame,
                area,
                EditOverlayArgs {
                    name,
                    desc,
                    extra_fields,
                    field: *field,
                    cursor: *cursor,
                    title: texts::OVERLAY_ADD_NODE,
                },
            );
        }
        InputMode::Editing {
            name,
            desc,
            extra_fields,
            field,
            cursor,
            ..
        } => {
            render_edit_overlay(
                frame,
                area,
                EditOverlayArgs {
                    name,
                    desc,
                    extra_fields,
                    field: *field,
                    cursor: *cursor,
                    title: texts::OVERLAY_EDIT_NODE,
                },
            );
        }
        InputMode::SelectFieldType { .. } => render_select_field_type_overlay(frame, area, state),
        InputMode::CustomFieldName { .. } => render_custom_field_name_overlay(frame, area, state),
        InputMode::ConfirmDeleteField { .. } => render_confirm_delete_overlay(frame, area, state),
        InputMode::ConfirmDiscardChanges { .. } => {
            render_confirm_discard_overlay(frame, area, state)
        }
        InputMode::Detail { .. } => {
            render_detail_overlay(frame, area, state);
            if matches!(state.mode, InputMode::Detail { ref alert, .. } if alert.is_some()) {
                render_copy_alert_overlay(frame, area);
            }
        }
        InputMode::ExportNodeSelect { .. } => {
            render_export_node_select_overlay(frame, area, state);
        }
        InputMode::ExportNodeFormat { .. } => {
            render_export_node_format_overlay(frame, area, state);
        }
        InputMode::Alert { message } => {
            render_alert_overlay(frame, area, message);
        }
        InputMode::Help => {
            render_help_overlay(frame, area);
        }
        _ => {}
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (mode_str, mode_style) = match &state.mode {
        InputMode::Normal => (texts::MODE_NORMAL, ElementStyle::ModeNormal.style()),
        InputMode::Search => (texts::MODE_SEARCH, ElementStyle::ModeSearch.style()),
        InputMode::AwaitingDd => (texts::MODE_AWAIT_DD, ElementStyle::ModeAwaitingDd.style()),
        InputMode::Detail { .. } => (texts::MODE_DETAIL, ElementStyle::ModeDetail.style()),
        InputMode::Editing { .. } => (texts::MODE_EDIT, ElementStyle::ModeEdit.style()),
        InputMode::Adding { .. } => (texts::MODE_ADD, ElementStyle::ModeAdd.style()),
        InputMode::SelectFieldType { .. } => {
            (texts::MODE_FIELD_TYPE, ElementStyle::ModeFieldType.style())
        }
        InputMode::CustomFieldName { .. } => (
            texts::MODE_CUSTOM_NAME,
            ElementStyle::ModeCustomName.style(),
        ),
        InputMode::ConfirmDeleteField { .. } => {
            (texts::MODE_CONFIRM, ElementStyle::ModeConfirmDelete.style())
        }
        InputMode::ConfirmDiscardChanges { .. } => {
            (texts::MODE_DISCARD, ElementStyle::ModeDiscard.style())
        }
        InputMode::ExportNodeSelect { .. } => (
            texts::MODE_EXPORT_SELECT,
            ElementStyle::ModeFieldType.style(),
        ),
        InputMode::ExportNodeFormat { .. } => (
            texts::MODE_EXPORT_FORMAT,
            ElementStyle::ModeFieldType.style(),
        ),
        InputMode::Alert { .. } => (texts::MODE_NORMAL, ElementStyle::ModeNormal.style()),
        InputMode::Help => (texts::MODE_HELP, ElementStyle::ModeHelp.style()),
    };

    let name = state.current_name().unwrap_or_default();

    let text = Text::from(Line::from(vec![
        Span::styled(
            format!(" {} ", mode_str),
            mode_style.add_modifier(Modifier::BOLD),
        ),
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
            ElementStyle::StatusBarText.style(),
        ),
    ]));

    frame.render_widget(Paragraph::new(text), area);
}

fn render_help_bar(frame: &mut Frame, area: Rect, state: &TreeState) {
    let help = match &state.mode {
        InputMode::Normal => texts::HINT_TREE,
        InputMode::Search => texts::HINT_SEARCH,
        InputMode::AwaitingDd => texts::HINT_AWAIT_DD,
        InputMode::Detail { ref alert, .. } if alert.is_some() => texts::HINT_DETAIL_ALERT,
        InputMode::Detail { .. } => texts::HINT_DETAIL,
        InputMode::Editing {
            extra_fields,
            field,
            ..
        } => {
            let is_on_extra = extra_field_index(*field).is_some_and(|i| i < extra_fields.len());
            if is_on_extra {
                texts::HINT_FORM_EXTRA
            } else if *field == 2 + extra_fields.len() {
                texts::HINT_FORM_ADD
            } else {
                texts::HINT_FORM
            }
        }
        InputMode::Adding {
            extra_fields,
            field,
            ..
        } => {
            let is_on_extra = extra_field_index(*field).is_some_and(|i| i < extra_fields.len());
            if is_on_extra {
                texts::HINT_FORM_EXTRA
            } else if *field == 2 + extra_fields.len() {
                texts::HINT_FORM_ADD
            } else {
                texts::HINT_FORM
            }
        }
        InputMode::SelectFieldType { .. } => texts::HINT_SELECT_FIELD_TYPE,
        InputMode::CustomFieldName { .. } => texts::HINT_CUSTOM_FIELD_NAME,
        InputMode::ConfirmDeleteField { .. } => texts::HINT_CONFIRM_DELETE,
        InputMode::ConfirmDiscardChanges { .. } => texts::HINT_DISCARD,
        InputMode::ExportNodeSelect { .. } => texts::HINT_EXPORT_NODE_SELECT,
        InputMode::ExportNodeFormat { .. } => texts::HINT_EXPORT_NODE_FORMAT,
        InputMode::Alert { .. } => texts::HINT_DETAIL_ALERT,
        InputMode::Help => texts::HINT_HELP,
    };

    let text = Text::from(Line::from(vec![Span::styled(
        help,
        ElementStyle::HelpBar.style(),
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
        .title(texts::OVERLAY_SEARCH)
        .borders(Borders::ALL)
        .style(ElementStyle::SearchBlock.style());

    let text = Text::from(Line::from(vec![Span::styled(
        format!("/{}", state.search_query),
        ElementStyle::SearchText.style(),
    )]));

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(text), inner);
}

fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let n_bindings = texts::TREE_HELP_KEY_BINDINGS.len() as u16;
    let overlay_height = (n_bindings + 4).min(area.height.saturating_sub(2));
    let overlay_area = Rect {
        x: area.width / 6,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width * 2 / 3,
        height: overlay_height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_HELP)
        .borders(Borders::ALL)
        .style(ElementStyle::HelpBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();
    for binding in texts::TREE_HELP_KEY_BINDINGS {
        lines.push(Line::from(Span::styled(
            *binding,
            ElementStyle::HelpText.style(),
        )));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_HELP,
        ElementStyle::DismissHint.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_export_node_select_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let selected = match &state.mode {
        InputMode::ExportNodeSelect { selected, .. } => *selected,
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 3,
        width: area.width / 2,
        height: 7,
    };

    let block = Block::default()
        .title(texts::OVERLAY_EXPORT_NODE)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in texts::EXPORT_NODE_OPTIONS.iter().enumerate() {
        lines.push(Line::from(selected_span(item, i == selected)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_EXPORT_NODE_SELECT,
        ElementStyle::GrayHint.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_export_node_format_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let selected_format = match &state.mode {
        InputMode::ExportNodeFormat {
            selected_format, ..
        } => *selected_format,
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 3,
        width: area.width / 2,
        height: 8,
    };

    let block = Block::default()
        .title(texts::OVERLAY_EXPORT_FORMAT)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in texts::EXPORT_TYPE_ITEMS.iter().enumerate() {
        lines.push(Line::from(selected_span(item, i == selected_format)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_EXPORT_NODE_FORMAT,
        ElementStyle::GrayHint.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_alert_overlay(frame: &mut Frame, area: Rect, message: &str) {
    let overlay_area = Rect {
        x: area.width / 6,
        y: area.height / 2 - 2,
        width: area.width * 2 / 3,
        height: 5,
    };

    let block = Block::default()
        .title(texts::ALERT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::CopiedBlock.style());

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", message),
            ElementStyle::CopiedText.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_DETAIL_ALERT,
            ElementStyle::DismissHint.style(),
        )),
    ];

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(&block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
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

fn render_extra_field_line(
    ef: &ExtraField,
    ef_index: usize,
    field: usize,
    cursor: usize,
    is_inactive_display: bool,
) -> Line<'static> {
    let is_active = field == ef_index + 2;
    let display_val = if is_active && !is_inactive_display {
        Cow::Borrowed(ef.value.as_str())
    } else {
        ef.field_type.display_value(&ef.value)
    };
    let prefix = format!("{}: ", ef.field_type.label());
    Line::from(cursor_field_spans(&prefix, &display_val, cursor, is_active))
}

/// Arguments for rendering the add/edit node overlay form.
struct EditOverlayArgs<'a> {
    name: &'a str,
    desc: &'a str,
    extra_fields: &'a [ExtraField],
    field: usize,
    cursor: usize,
    title: &'a str,
}

fn render_edit_overlay(frame: &mut Frame, area: Rect, args: EditOverlayArgs<'_>) {
    let EditOverlayArgs {
        name,
        desc,
        extra_fields,
        field,
        cursor,
        title,
    } = args;
    let overlay_area = make_overlay_area(area, extra_fields.len(), 5);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .style(ElementStyle::EditBlock.style());

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(cursor_field_spans(
        texts::LABEL_NAME,
        name,
        cursor,
        field == 0,
    )));
    lines.push(Line::from(cursor_field_spans(
        texts::LABEL_DESC,
        desc,
        cursor,
        field == 1,
    )));

    for (i, ef) in extra_fields.iter().enumerate() {
        lines.push(render_extra_field_line(ef, i, field, cursor, false));
    }

    let is_add_active = field == 2 + extra_fields.len();
    let add_style = if is_add_active {
        ElementStyle::AddButtonActive.style()
    } else {
        ElementStyle::AddButton.style()
    };
    lines.push(Line::from(Span::styled(texts::ADD_FIELD_BUTTON, add_style)));
    lines.push(Line::from(""));

    let hint = if field >= 2 && field < 2 + extra_fields.len() {
        texts::HINT_FORM_EXTRA
    } else if field == 2 + extra_fields.len() {
        texts::HINT_FORM_ADD
    } else {
        texts::HINT_FORM
    };
    lines.push(Line::from(Span::styled(
        hint,
        ElementStyle::GrayHint.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
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
        .title(texts::OVERLAY_SELECT_FIELD_TYPE)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in texts::FIELD_TYPE_ITEMS.iter().enumerate() {
        lines.push(Line::from(selected_span(item, i == selected_type)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_SELECT_FIELD_TYPE,
        ElementStyle::GrayHint.style(),
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
        .title(texts::OVERLAY_CUSTOM_FIELD_NAME)
        .borders(Borders::ALL)
        .style(ElementStyle::CustomNameBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(cursor_field_spans(
            texts::LABEL_NAME,
            &input,
            input.len(),
            true,
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_CUSTOM_FIELD_NAME,
            ElementStyle::GrayHint.style(),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_confirm_delete_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let label = match &state.mode {
        InputMode::ConfirmDeleteField {
            ref extra_fields,
            target_index,
            ..
        } => extra_fields
            .get(*target_index)
            .map(|ef| ef.field_type.label())
            .unwrap_or_default(),
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 2,
        width: area.width / 2,
        height: 5,
    };

    let block = Block::default()
        .title(texts::OVERLAY_CONFIRM_DELETE)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(Span::styled(
            format!(
                "{}{}{}",
                texts::PROMPT_DELETE_FIELD_START,
                label,
                texts::PROMPT_DELETE_FIELD_END
            ),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_CONFIRM_DELETE,
            ElementStyle::DismissHint.style(),
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
        .title(texts::OVERLAY_COPIED)
        .borders(Borders::ALL)
        .style(ElementStyle::CopiedBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::COPY_CHECK,
            ElementStyle::CopiedText.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_DETAIL_ALERT,
            ElementStyle::DismissHint.style(),
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
        .title(texts::OVERLAY_UNSAVED_CHANGES)
        .borders(Borders::ALL)
        .style(ElementStyle::DiscardBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::PROMPT_DISCARD,
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_DISCARD,
            ElementStyle::DismissHint.style(),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_detail_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let node = match state.current_node_ref() {
        Some(n) => n,
        None => return,
    };

    let selected = match &state.mode {
        InputMode::Detail { selected, .. } => *selected,
        _ => 0,
    };

    let overlay_area = make_overlay_area(area, node.extra_fields.len(), 4);

    let block = Block::default()
        .title(format!(" {} ", node.name))
        .borders(Borders::ALL)
        .style(ElementStyle::DetailBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    let desc = if node.description.is_empty() {
        texts::NO_DESCRIPTION
    } else {
        node.description.as_str()
    };
    lines.push(Line::from(selected_span(desc, selected == 0)));

    for (i, ef) in node.extra_fields.iter().enumerate() {
        let ef_index = i + 1;
        let display_val = ef.field_type.display_value(&ef.value);
        let is_sel = selected == ef_index;
        let text = format!("{}: {}", ef.field_type.label(), display_val);
        lines.push(Line::from(selected_span(&text, is_sel)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_DETAIL,
        ElementStyle::DismissHint.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}
