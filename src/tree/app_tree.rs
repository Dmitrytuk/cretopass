use std::borrow::Cow;
use std::io::{self, Write};

use crate::cursor::{
    backspace_at, cursor_field_spans, cursor_left, cursor_right, delete_at, insert_char_at,
    selected_span,
};
use crate::file_handling::save_tree;
use crate::theming::Action;
use crate::theming::{ElementStyle, Icon, texts};
use crate::tree::node::{ExtraField, FieldType};
use crate::tree::widget::{
    AddPosition, FormFocus, FormPending, FormRow, FormState, InputMode, TreeState, TreeWidget,
};
use crossterm::event::{self, Event, KeyEventKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Terminal, backend::Backend};

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
                InputMode::PendingCut => handle_pending_cut(key, state),
                InputMode::PendingPaste => handle_pending_paste(key, state),
                InputMode::PendingAdd => handle_pending_add(key, state),
                InputMode::Form(_) => handle_form(key, state),
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

            if state.modified {
                state.status = Some(texts::STATUS_SAVING);
                terminal.draw(|f| ui(f, state))?;
                if let Some(ref p) = state.active_vault_path {
                    state.modified = false;
                    if let Err(e) = save_tree(&state.root, p, &state.active_vault_master_password) {
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
            state.collapse_or_select_parent();
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
            state.mode = InputMode::PendingCut;
        }
        Action::PasteStart => {
            state.mode = InputMode::PendingPaste;
        }
        Action::AddStart => {
            state.mode = InputMode::PendingAdd;
        }
        Action::OpenDetail => {
            state.mode = InputMode::Detail {
                selected: 0,
                alert: None,
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

fn handle_pending_cut(key: event::KeyEvent, state: &mut TreeState) {
    if Action::from_pending_cut(&key) == Action::Cut {
        state.cut_selected();
    }
    state.mode = InputMode::Normal;
}

fn handle_pending_paste(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_pending_paste(&key) {
        Action::PasteSibling => {
            state.paste_sibling();
            state.mode = InputMode::Normal;
        }
        Action::PasteBefore => {
            state.paste_sibling_before();
            state.mode = InputMode::Normal;
        }
        Action::PasteChild => {
            state.paste_child();
            state.mode = InputMode::Normal;
        }
        _ => {
            state.mode = InputMode::Normal;
        }
    }
}

fn handle_pending_add(key: event::KeyEvent, state: &mut TreeState) {
    let position = match Action::from_pending_add(&key) {
        Action::AddSiblingNode => AddPosition::SiblingAfter,
        Action::AddBefore => AddPosition::SiblingBefore,
        Action::AddNode => AddPosition::Child,
        _ => {
            state.mode = InputMode::Normal;
            return;
        }
    };
    state.add_position = position;
    state.mode = InputMode::Form(FormState::new_add());
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

/// An entry offered by the field-type picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerItem {
    Login,
    Password,
    Custom,
    /// Enables the description row (Add mode only, until it is shown).
    Desc,
}

impl PickerItem {
    fn label(self) -> &'static str {
        match self {
            Self::Login => texts::FIELD_LOGIN,
            Self::Password => texts::FIELD_PASSWORD,
            Self::Custom => texts::FIELD_ITEM_CUSTOM,
            Self::Desc => texts::FIELD_ITEM_DESC,
        }
    }
}

/// Applies the given edit to the form's focused text and updates the cursor.
fn form_edit<F>(form: &mut FormState, edit: F)
where
    F: FnOnce(&mut String, &mut usize),
{
    let mut cursor = form.cursor;
    if let Some(text) = form.active_text_mut() {
        edit(text, &mut cursor);
    }
    form.cursor = cursor;
}

fn handle_form(key: event::KeyEvent, state: &mut TreeState) {
    let mut form = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::Form(form) => form,
        other => {
            state.mode = other;
            return;
        }
    };

    form.clamp_selection();
    let row = form.current_row();

    let action = match form.pending {
        FormPending::Cut => Action::from_form_pending(&key, false),
        FormPending::Paste => Action::from_form_pending(&key, true),
        FormPending::None => match form.focus {
            FormFocus::Nav => Action::from_form_nav(&key),
            FormFocus::Input => Action::from_form_input(&key),
        },
    };

    // Any unmapped key while a field prefix is pending cancels just the prefix.
    if form.pending != FormPending::None && action == Action::Cancel {
        form.pending = FormPending::None;
        state.mode = InputMode::Form(form);
        return;
    }

    match action {
        Action::Cut if form.pending == FormPending::None => {
            if matches!(row, FormRow::Extra(_)) {
                form.pending = FormPending::Cut;
            }
        }
        Action::Cut => {
            form.cut_focused_field();
            form.pending = FormPending::None;
        }
        Action::PasteStart => {
            if form.field_cut_buffer.is_some() && matches!(row, FormRow::Extra(_)) {
                form.pending = FormPending::Paste;
            }
        }
        Action::PasteSibling => {
            form.paste_field_after();
            form.pending = FormPending::None;
        }
        Action::PasteBefore => {
            form.paste_field_before();
            form.pending = FormPending::None;
        }
        Action::BeginInput => {
            if row != FormRow::AddButton {
                form.focus = FormFocus::Input;
            }
        }
        Action::ExitInput => {
            form.focus = FormFocus::Nav;
        }
        // Delete on an extra-field row opens the confirm-delete popup.
        Action::DeleteChar if row == FormRow::AddButton => {}
        Action::DeleteChar
            if matches!(row, FormRow::Name | FormRow::Desc) && form.focus == FormFocus::Nav => {}
        Action::Confirm if row == FormRow::AddButton => {
            state.mode = InputMode::SelectFieldType { form, selected: 0 };
            return;
        }
        Action::Confirm => {
            save_form(state, form);
            return;
        }
        Action::Cancel => {
            let has_changes = if form.is_adding() {
                adding_has_changes(&form)
            } else {
                editing_has_changes(state, &form)
            };
            if has_changes {
                state.mode = InputMode::ConfirmDiscardChanges(form);
            } else if form.is_adding() {
                state.mode = InputMode::Normal;
            } else {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            }
            return;
        }
        Action::NextField => {
            let len = form.rows().len();
            let next = (form.selected_row + 1) % len;
            form.select_row(next);
        }
        Action::PrevField => {
            let len = form.rows().len();
            let prev = (form.selected_row + len - 1) % len;
            form.select_row(prev);
        }
        Action::CursorLeft => {
            form_edit(&mut form, |text, cursor| {
                *cursor = cursor_left(text, *cursor)
            });
        }
        Action::CursorRight => {
            form_edit(&mut form, |text, cursor| {
                *cursor = cursor_right(text, *cursor)
            });
        }
        Action::CursorHome => {
            form_edit(&mut form, |_, cursor| *cursor = 0);
        }
        Action::CursorEnd => {
            let len = form.active_len();
            form_edit(&mut form, |_, cursor| *cursor = len);
        }
        Action::Backspace => {
            if form.cursor > 0 {
                form_edit(&mut form, |text, cursor| {
                    *cursor = backspace_at(text, *cursor);
                });
            }
        }
        Action::DeleteChar => {
            if let FormRow::Extra(i) = row
                && form.focus == FormFocus::Nav
            {
                state.mode = InputMode::ConfirmDeleteField {
                    form,
                    target_index: i,
                };
                return;
            }
            if form.focus == FormFocus::Input {
                form_edit(&mut form, |text, cursor| delete_at(text, *cursor));
            }
        }
        Action::InsertChar(c) => {
            form_edit(&mut form, |text, cursor| insert_char_at(text, cursor, c));
        }
        _ => {}
    }

    state.mode = InputMode::Form(form);
}

fn save_form(state: &mut TreeState, form: FormState) {
    if form.is_adding() {
        match state.add_position {
            AddPosition::SiblingAfter => {
                state.add_sibling_to_selected(form.name, form.desc, form.extra_fields);
            }
            AddPosition::SiblingBefore => {
                state.add_sibling_before_to_selected(form.name, form.desc, form.extra_fields);
            }
            AddPosition::Child => {
                state.add_child_to_selected(form.name, form.desc, form.extra_fields);
            }
        }
        state.mode = InputMode::Normal;
    } else {
        state.update_node(
            form.node_id.unwrap(),
            form.name,
            form.desc,
            form.extra_fields,
        );
        state.mode = InputMode::Detail {
            selected: 0,
            alert: None,
        };
    }
}

fn handle_select_field_type(key: event::KeyEvent, state: &mut TreeState) {
    let (mut form, selected) = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::SelectFieldType { form, selected } => (form, selected),
        other => {
            state.mode = other;
            return;
        }
    };

    let items = picker_items(&form);
    let selected = selected.min(items.len().saturating_sub(1));

    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.mode = InputMode::Form(form);
        }
        Action::NavDown => {
            let next = (selected + 1) % items.len();
            state.mode = InputMode::SelectFieldType {
                form,
                selected: next,
            };
        }
        Action::NavUp => {
            let prev = (selected + items.len() - 1) % items.len();
            state.mode = InputMode::SelectFieldType {
                form,
                selected: prev,
            };
        }
        Action::SelectIndex(i) => {
            if i < items.len() {
                state.mode = InputMode::SelectFieldType { form, selected: i };
            } else {
                state.mode = InputMode::SelectFieldType { form, selected };
            }
        }
        Action::Confirm => match items[selected] {
            PickerItem::Login => {
                form.extra_fields.push(ExtraField {
                    field_type: FieldType::Login,
                    value: String::new(),
                });
                form.selected_row = form.rows().len() - 2; // the new field, before AddButton
                form.focus = FormFocus::Input;
                form.cursor = 0;
                state.mode = InputMode::Form(form);
            }
            PickerItem::Password => {
                form.extra_fields.push(ExtraField {
                    field_type: FieldType::Password,
                    value: String::new(),
                });
                form.selected_row = form.rows().len() - 2;
                form.focus = FormFocus::Input;
                form.cursor = 0;
                state.mode = InputMode::Form(form);
            }
            PickerItem::Custom => {
                state.mode = InputMode::CustomFieldName {
                    form,
                    input: String::new(),
                };
            }
            PickerItem::Desc => {
                form.show_desc = true;
                form.selected_row = 1; // Name, Desc, ...
                form.focus = FormFocus::Input;
                form.cursor = form.desc.len();
                state.mode = InputMode::Form(form);
            }
        },
        _ => {
            state.mode = InputMode::SelectFieldType { form, selected };
        }
    }
}

/// Items offered by the field-type picker for the given form.
fn picker_items(form: &FormState) -> Vec<PickerItem> {
    let mut items = vec![PickerItem::Login, PickerItem::Password, PickerItem::Custom];
    if !form.show_desc {
        items.push(PickerItem::Desc);
    }
    items
}

fn handle_custom_field_name(key: event::KeyEvent, state: &mut TreeState) {
    let (mut form, input) = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::CustomFieldName { form, input } => (form, input),
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_custom_field_name(&key) {
        Action::Cancel => {
            state.mode = InputMode::SelectFieldType { form, selected: 2 };
        }
        Action::Confirm => {
            let label = if input.is_empty() {
                texts::CUSTOM_FIELD_DEFAULT.to_string()
            } else {
                input
            };
            form.extra_fields.push(ExtraField {
                field_type: FieldType::Custom(label),
                value: String::new(),
            });
            form.selected_row = form.rows().len() - 2;
            form.focus = FormFocus::Input;
            form.cursor = 0;
            state.mode = InputMode::Form(form);
        }
        Action::Backspace => {
            let mut new_input = input;
            new_input.pop();
            state.mode = InputMode::CustomFieldName {
                form,
                input: new_input,
            };
        }
        Action::InsertChar(c) => {
            let mut new_input = input;
            new_input.push(c);
            state.mode = InputMode::CustomFieldName {
                form,
                input: new_input,
            };
        }
        _ => {
            state.mode = InputMode::CustomFieldName { form, input };
        }
    }
}

fn handle_confirm_delete(key: event::KeyEvent, state: &mut TreeState) {
    let (mut form, target_index) = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::ConfirmDeleteField { form, target_index } => (form, target_index),
        other => {
            state.mode = other;
            return;
        }
    };

    if Action::from_confirm(&key) == Action::Yes {
        if target_index < form.extra_fields.len() {
            form.extra_fields.remove(target_index);
        }
        form.focus = FormFocus::Nav;
        form.clamp_selection();
        form.cursor = form.active_len();
    }
    state.mode = InputMode::Form(form);
}

fn editing_has_changes(state: &TreeState, form: &FormState) -> bool {
    let Some(node_id) = form.node_id else {
        return false;
    };
    let original = state.root.find(node_id);
    match original {
        None => false,
        Some(n) => {
            if form.name != n.name || form.desc != n.description {
                return true;
            }
            if form.extra_fields.len() != n.extra_fields.len() {
                return true;
            }
            for (a, b) in form.extra_fields.iter().zip(n.extra_fields.iter()) {
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

fn adding_has_changes(form: &FormState) -> bool {
    !form.name.is_empty() || !form.desc.is_empty() || !form.extra_fields.is_empty()
}

fn handle_confirm_discard(key: event::KeyEvent, state: &mut TreeState) {
    let form = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::ConfirmDiscardChanges(form) => form,
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_confirm(&key) {
        Action::Yes => {
            if form.node_id.is_some() {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            } else {
                state.mode = InputMode::Normal;
            }
        }
        Action::No => {
            state.mode = InputMode::Form(form);
        }
        _ => {
            state.mode = InputMode::ConfirmDiscardChanges(form);
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
            if let Some(ref t) = text
                && !t.is_empty()
            {
                copy_to_clipboard(t);
            }
            state.mode = InputMode::Detail {
                selected,
                alert: Some(texts::COPY_CHECK.to_string()),
            };
        }
        Action::EditNode => {
            if let Some(node) = state.current_node_ref() {
                let form = FormState::new_edit(node);
                state.mode = InputMode::Form(form);
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
        Action::SelectIndex(i) if i < 2 => {
            state.mode = InputMode::ExportNodeSelect { node, selected: i };
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
        Action::SelectIndex(i) if i < 3 => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: i,
            };
        }
        Action::Confirm => {
            use crate::file_handling::export::{ExportFormat, export_tree};

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

            let base_path = match &state.active_vault_path {
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
        InputMode::Form(form) => {
            let title = if form.is_adding() {
                texts::OVERLAY_ADD_NODE
            } else {
                texts::OVERLAY_EDIT_NODE
            };
            render_edit_overlay(frame, area, form, title);
        }
        InputMode::SelectFieldType { .. } => render_select_field_type_overlay(frame, area, state),
        InputMode::CustomFieldName { .. } => render_custom_field_name_overlay(frame, area, state),
        InputMode::ConfirmDeleteField { .. } => render_confirm_delete_overlay(frame, area, state),
        InputMode::ConfirmDiscardChanges(_) => render_confirm_discard_overlay(frame, area, state),
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
        InputMode::PendingCut => (
            texts::MODE_PENDING_CUT,
            ElementStyle::ModePendingCut.style(),
        ),
        InputMode::PendingPaste => (texts::MODE_PASTE, ElementStyle::ModePendingPaste.style()),
        InputMode::PendingAdd => (
            texts::MODE_PENDING_ADD,
            ElementStyle::ModePendingAdd.style(),
        ),
        InputMode::Detail { .. } => (texts::MODE_DETAIL, ElementStyle::ModeDetail.style()),
        InputMode::Form(form) => match form.pending {
            FormPending::Cut => (
                texts::MODE_PENDING_CUT,
                ElementStyle::ModePendingCut.style(),
            ),
            FormPending::Paste => (texts::MODE_PASTE, ElementStyle::ModePendingPaste.style()),
            FormPending::None if form.is_adding() => {
                (texts::MODE_ADD, ElementStyle::ModeAdd.style())
            }
            FormPending::None => (texts::MODE_EDIT, ElementStyle::ModeEdit.style()),
        },
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
        InputMode::PendingCut => texts::HINT_PENDING_CUT,
        InputMode::PendingPaste => texts::HINT_PASTE,
        InputMode::PendingAdd => texts::HINT_PENDING_ADD,
        InputMode::Detail { alert, .. } if alert.is_some() => texts::HINT_DETAIL_ALERT,
        InputMode::Detail { .. } => texts::HINT_DETAIL,
        InputMode::Form(form) => form_hint(form),
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

/// Renders one form row as a list-style line.
///
/// - Nav: the focused row gets `▸ ` + `FormRowFocused` (like Detail); other
///   rows get `  ` + dimmed label/text.
/// - Input: the focused row shows the inverted cursor; other rows are dimmed.
fn form_row_line(
    label: &str,
    text: &str,
    cursor: usize,
    is_focused: bool,
    is_editing: bool,
) -> Line<'static> {
    let prefix = if is_focused {
        Icon::SelectionArrow.as_str()
    } else {
        Icon::SelectionPad.as_str()
    };

    if is_focused && is_editing {
        let mut spans = vec![Span::raw(prefix.to_string())];
        spans.extend(cursor_field_spans(label, text, cursor, true));
        return Line::from(spans);
    }

    let style = if is_focused {
        ElementStyle::FormRowFocused.style()
    } else {
        ElementStyle::TreeRow.style()
    };
    Line::from(vec![Span::styled(format!("{prefix}{label}{text}"), style)])
}

/// Renders one extra-field row, masking password values unless the row is
/// actively being edited (`Input` level).
fn render_extra_field_line(
    ef: &ExtraField,
    is_focused: bool,
    cursor: usize,
    is_editing: bool,
) -> Line<'static> {
    let revealed = is_focused && is_editing;
    let display_val = if revealed {
        Cow::Borrowed(ef.value.as_str())
    } else {
        ef.field_type.display_value(&ef.value)
    };
    let label = format!("{}: ", ef.field_type.label());
    form_row_line(&label, &display_val, cursor, is_focused, is_editing)
}

fn render_edit_overlay(frame: &mut Frame, area: Rect, form: &FormState, title: &str) {
    let rows = form.rows();
    let current = form.selected_row.min(rows.len() - 1);
    let is_editing = form.focus == FormFocus::Input;

    let overlay_area = make_overlay_area(area, form.extra_fields.len(), 5);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .style(ElementStyle::EditBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        let focused = i == current;
        match row {
            FormRow::Name => lines.push(form_row_line(
                texts::LABEL_NAME,
                &form.name,
                form.cursor,
                focused,
                is_editing,
            )),
            FormRow::Desc => lines.push(form_row_line(
                texts::LABEL_DESC,
                &form.desc,
                form.cursor,
                focused,
                is_editing,
            )),
            FormRow::Extra(idx) => {
                if let Some(ef) = form.extra_fields.get(*idx) {
                    lines.push(render_extra_field_line(
                        ef,
                        focused,
                        form.cursor,
                        is_editing,
                    ));
                }
            }
            FormRow::AddButton => {
                let style = if focused {
                    ElementStyle::FormRowFocused.style()
                } else {
                    ElementStyle::TreeRow.style()
                };
                let prefix = if focused {
                    Icon::SelectionArrow.as_str()
                } else {
                    Icon::SelectionPad.as_str()
                };
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{}", texts::ADD_FIELD_BUTTON),
                    style,
                )));
            }
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        form_hint(form),
        ElementStyle::GrayHint.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Selects the help-bar hint for the Add/Edit form.
fn form_hint(form: &FormState) -> &'static str {
    match form.pending {
        FormPending::Cut => return texts::HINT_FIELD_CUT,
        FormPending::Paste => return texts::HINT_FIELD_PASTE,
        FormPending::None => {}
    }
    if form.focus == FormFocus::Input {
        return texts::HINT_FORM_INPUT;
    }
    match form.current_row() {
        FormRow::AddButton => texts::HINT_FORM_ADD,
        FormRow::Extra(_) => texts::HINT_FORM_EXTRA,
        FormRow::Name | FormRow::Desc => texts::HINT_FORM,
    }
}

fn render_select_field_type_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (form, selected_type) = match &state.mode {
        InputMode::SelectFieldType { form, selected } => (form, *selected),
        _ => return,
    };
    let items = picker_items(form);

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

    for (i, item) in items.iter().enumerate() {
        let label = format!("{}. {}", i + 1, item.label());
        lines.push(Line::from(selected_span(&label, i == selected_type)));
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
        InputMode::ConfirmDeleteField { form, target_index } => form
            .extra_fields
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::Node;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn editing_state() -> TreeState {
        let root = Node::new("Root", "");
        let mut state = TreeState::new(root);
        let node = state.root.clone();
        state.mode = InputMode::Form(FormState::new_edit(&node));
        state
    }

    fn form_of(state: &TreeState) -> &FormState {
        match &state.mode {
            InputMode::Form(form) => form,
            other => panic!("expected Form mode, got {other:?}"),
        }
    }

    #[test]
    fn form_opens_in_nav_and_i_enters_input() {
        let mut state = editing_state();
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        handle_form(key(KeyCode::Char('i')), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Input);
    }

    #[test]
    fn form_nav_j_moves_row_to_extra_or_add_button() {
        let mut node = Node::new("Root", "d");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: "x".to_string(),
        });
        let mut state = TreeState::new(node.clone());
        state.mode = InputMode::Form(FormState::new_edit(&node));
        handle_form(key(KeyCode::Char('j')), &mut state);
        let form = form_of(&state);
        assert_eq!(form.name, "Root");
        assert_eq!(form.selected_row, 1); // moved to Extra(0) (desc hidden)
        assert_eq!(form.current_row(), FormRow::Extra(0));
    }

    #[test]
    fn form_input_esc_returns_to_nav() {
        let mut state = editing_state();
        if let InputMode::Form(ref mut form) = state.mode {
            form.focus = FormFocus::Input;
        }
        handle_form(key(KeyCode::Esc), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        assert!(matches!(state.mode, InputMode::Form(_)));
    }

    #[test]
    fn form_nav_esc_with_changes_asks_to_discard() {
        let mut state = editing_state();
        if let InputMode::Form(ref mut form) = state.mode {
            form.name = "changed".to_string();
        }
        handle_form(key(KeyCode::Esc), &mut state);
        assert!(matches!(state.mode, InputMode::ConfirmDiscardChanges(_)));
    }

    #[test]
    fn form_input_typing_inserts_char() {
        let mut state = editing_state();
        if let InputMode::Form(ref mut form) = state.mode {
            form.focus = FormFocus::Input;
            form.cursor = form.name.len();
        }
        handle_form(key(KeyCode::Char('j')), &mut state);
        assert_eq!(form_of(&state).name, "Rootj");
    }

    #[test]
    fn add_form_starts_with_only_name_and_add_button() {
        let form = FormState::new_add();
        assert_eq!(form.rows(), vec![FormRow::Name, FormRow::AddButton]);
        assert!(!form.show_desc);
    }

    #[test]
    fn edit_form_hides_desc_until_added() {
        let mut node = Node::new("n", "d");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: String::new(),
        });
        let form = FormState::new_edit(&node);
        // desc is hidden until explicitly added via the field-type picker.
        assert!(!form.show_desc);
        assert_eq!(
            form.rows(),
            vec![FormRow::Name, FormRow::Extra(0), FormRow::AddButton]
        );
    }

    #[test]
    fn adding_desc_enables_the_desc_row() {
        let mut state = TreeState::new(Node::new("Root", ""));
        state.add_position = AddPosition::Child;
        state.mode = InputMode::Form(FormState::new_add());
        // Position the selection on the add button and open the picker.
        if let InputMode::Form(ref mut form) = state.mode {
            form.select_row(1);
        }
        handle_form(key(KeyCode::Enter), &mut state);
        assert!(matches!(state.mode, InputMode::SelectFieldType { .. }));

        // Description is offered last for a new node without desc.
        let items = match &state.mode {
            InputMode::SelectFieldType { form, .. } => picker_items(form),
            _ => unreachable!(),
        };
        assert_eq!(items.last(), Some(&PickerItem::Desc));

        // Choose it and verify the row appears.
        let desc_index = items.len() - 1;
        let mut state2 = state;
        if let InputMode::SelectFieldType {
            ref mut selected, ..
        } = state2.mode
        {
            *selected = desc_index;
        }
        handle_select_field_type(key(KeyCode::Enter), &mut state2);
        let form = form_of(&state2);
        assert!(form.show_desc);
        assert_eq!(form.current_row(), FormRow::Desc);
    }

    #[test]
    fn password_is_masked_until_input_level() {
        let ef = ExtraField {
            field_type: FieldType::Password,
            value: "secret".to_string(),
        };
        // Nav: masked.
        let line = render_extra_field_line(&ef, true, 0, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("secret"));
        assert!(text.contains(texts::PASSWORD_MASK));

        // Input: revealed.
        let line = render_extra_field_line(&ef, true, 0, true);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("secret"));
    }

    fn form_with_fields() -> FormState {
        let mut node = Node::new("n", "");
        for value in ["a", "b", "c"] {
            node.extra_fields.push(ExtraField {
                field_type: FieldType::Login,
                value: value.to_string(),
            });
        }
        FormState::new_edit(&node)
    }

    fn values(form: &FormState) -> Vec<&str> {
        form.extra_fields.iter().map(|f| f.value.as_str()).collect()
    }

    fn select_extra(form: &mut FormState, extra_index: usize) {
        // rows: Name, [Desc,] Extra(0..), AddButton
        let desc_offset = usize::from(form.show_desc);
        form.select_row(1 + desc_offset + extra_index);
    }

    #[test]
    fn field_cut_requires_two_d_presses() {
        let mut state = TreeState::new(Node::new("Root", ""));
        let mut form = form_with_fields();
        select_extra(&mut form, 1);
        state.mode = InputMode::Form(form);

        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::Cut);
        assert_eq!(form_of(&state).extra_fields.len(), 3);

        handle_form(key(KeyCode::Char('d')), &mut state);
        let form = form_of(&state);
        assert_eq!(form.pending, FormPending::None);
        assert_eq!(values(form), vec!["a", "c"]);
        assert_eq!(form.field_cut_buffer.as_ref().unwrap().value, "b");
    }

    #[test]
    fn field_cut_is_cancelled_by_other_keys() {
        let mut state = TreeState::new(Node::new("Root", ""));
        let mut form = form_with_fields();
        select_extra(&mut form, 0);
        state.mode = InputMode::Form(form);

        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Esc), &mut state);
        let form = form_of(&state);
        assert_eq!(form.pending, FormPending::None);
        assert_eq!(form.extra_fields.len(), 3);
        assert!(matches!(state.mode, InputMode::Form(_)));
    }

    #[test]
    fn field_cut_is_noop_on_name_row() {
        let mut state = TreeState::new(Node::new("Root", ""));
        let mut form = form_with_fields();
        form.select_row(0);
        state.mode = InputMode::Form(form);

        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::None);
    }

    #[test]
    fn field_paste_after_moves_field_down() {
        let mut state = TreeState::new(Node::new("Root", ""));
        let mut form = form_with_fields();
        select_extra(&mut form, 0); // "a"
        state.mode = InputMode::Form(form);

        // Cut "a", then select "c" and paste after it.
        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        if let InputMode::Form(ref mut form) = state.mode {
            select_extra(form, 1); // "c" now at index 1
        }
        handle_form(key(KeyCode::Char('p')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::Paste);
        handle_form(key(KeyCode::Char('p')), &mut state);

        let form = form_of(&state);
        assert_eq!(values(form), vec!["b", "c", "a"]);
        assert!(form.field_cut_buffer.is_none());
        assert_eq!(form.current_row(), FormRow::Extra(2));
    }

    #[test]
    fn field_paste_before_moves_field_up() {
        let mut state = TreeState::new(Node::new("Root", ""));
        let mut form = form_with_fields();
        select_extra(&mut form, 2); // "c"
        state.mode = InputMode::Form(form);

        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        if let InputMode::Form(ref mut form) = state.mode {
            select_extra(form, 0); // "a"
        }
        handle_form(key(KeyCode::Char('p')), &mut state);
        handle_form(key(KeyCode::Char('P')), &mut state);

        let form = form_of(&state);
        assert_eq!(values(form), vec!["c", "a", "b"]);
        assert_eq!(form.current_row(), FormRow::Extra(0));
    }

    #[test]
    fn paste_without_buffer_does_nothing() {
        let mut state = TreeState::new(Node::new("Root", ""));
        let mut form = form_with_fields();
        select_extra(&mut form, 0);
        state.mode = InputMode::Form(form);

        handle_form(key(KeyCode::Char('p')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::None);
        assert_eq!(form_of(&state).extra_fields.len(), 3);
    }

    #[test]
    fn confirm_delete_removes_the_field_and_clamps_selection() {
        let mut node = Node::new("n", "");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: "a".to_string(),
        });
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: "b".to_string(),
        });
        let mut initial = FormState::new_edit(&node);
        initial.select_row(3); // second extra field
        let mut state = TreeState::new(Node::new("Root", ""));
        state.mode = InputMode::ConfirmDeleteField {
            form: initial,
            target_index: 1,
        };
        handle_confirm_delete(key(KeyCode::Char('y')), &mut state);
        let updated = form_of(&state);
        assert_eq!(updated.extra_fields.len(), 1);
        assert!(updated.selected_row < updated.rows().len());
    }
}
