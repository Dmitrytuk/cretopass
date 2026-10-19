use std::borrow::Cow;
use std::io::{self, Write};

use crate::cursor::{
    backspace_at, cursor_field_spans, cursor_left, cursor_right, delete_at, insert_char_at,
    selected_span,
};
use crate::file_handling::{Autosave, SaveError, SaveEvent, save_tree};
use crate::theming::Action;
use crate::theming::{ElementStyle, Icon, texts};
use crate::tree::node::{ExtraField, FieldType, Node};
use crate::tree::widget::{
    AddPosition, FormFocus, FormPending, FormRow, FormState, InputMode, TreeState, TreeWidget,
};
use crossterm::event::{self, Event, EventStream, KeyEventKind};
use futures_util::StreamExt;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

/// Runs the tree screen until the user quits or returns to the home screen.
///
/// Committed changes are handed to a background save worker; `passphrase`
/// supplies the vault credential for that worker. On exit the function waits
/// for the latest snapshot to be written, so `Ok(())` means the vault is up to
/// date.
///
/// # Errors
///
/// Returns an I/O error when the terminal or event stream fails, or when the
/// save worker cannot be started.
pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
    passphrase: Option<Zeroizing<Vec<u8>>>,
) -> io::Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    let (save_tx, mut save_rx) = mpsc::unbounded_channel::<SaveEvent>();
    let mut autosave = start_autosave(state, passphrase, save_tx)?;
    let has_worker = autosave.is_some();

    let mut submitted: u64 = 0;
    let mut saved: u64 = 0;
    let mut save_open = has_worker;

    runtime.block_on(async {
        let mut events = EventStream::new();
        terminal.draw(|f| ui(f, state))?;

        loop {
            tokio::select! {
                maybe_event = events.next() => {
                    match maybe_event {
                        Some(Ok(Event::Key(key))) => {
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            if state.save_error.take().is_some() {
                                terminal.draw(|f| ui(f, state))?;
                                continue;
                            }

                            dispatch_key(key, state);

                            if state.modified {
                                state.modified = false;
                                if let Some(ref mut worker) = autosave {
                                    submitted = worker.submit(state.root.clone());
                                    state.status = Some(texts::STATUS_SAVING);
                                }
                            }
                        }
                        Some(Ok(Event::Resize(_, _))) => {}
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => return Err(e),
                        None => {
                            return Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "terminal event stream ended",
                            ));
                        }
                    }
                }
                event = save_rx.recv(), if save_open => {
                    match event {
                        Some(event) => {
                            if event.result.is_ok() {
                                saved = saved.max(event.generation);
                            }
                            if event.generation == submitted {
                                match event.result {
                                    Ok(()) => {
                                        state.status = None;
                                        state.save_error = None;
                                    }
                                    Err(error) => {
                                        state.status = Some(texts::STATUS_SAVE_FAILED);
                                        state.save_error =
                                            Some(format!("{}{}", texts::MSG_SAVE_FAILED, error));
                                    }
                                }
                            }
                        }
                        None => {
                            save_open = false;
                            if saved < submitted {
                                state.status = Some(texts::STATUS_SAVE_FAILED);
                                state.save_error = Some(format!(
                                    "{}{}",
                                    texts::MSG_SAVE_FAILED,
                                    SaveError::WorkerStopped
                                ));
                            }
                        }
                    }
                }
            }

            if state.quit || state.return_to_menu {
                if let Some(ref mut worker) = autosave {
                    let retry = state.modified || state.status == Some(texts::STATUS_SAVE_FAILED);
                    if retry {
                        state.modified = false;
                        submitted = worker.submit(state.root.clone());
                    }

                    if saved < submitted {
                        state.status = Some(texts::STATUS_SAVING);
                        terminal.draw(|f| ui(f, state))?;

                        match wait_for_save(&mut save_rx, state, &mut saved, submitted).await {
                            Ok(()) => return Ok(()),
                            Err(error) => {
                                state.quit = false;
                                state.return_to_menu = false;
                                state.status = Some(texts::STATUS_SAVE_FAILED);
                                state.save_error =
                                    Some(format!("{}{}", texts::MSG_SAVE_FAILED, error));
                            }
                        }
                    } else {
                        return Ok(());
                    }
                } else {
                    return Ok(());
                }
            }

            terminal.draw(|f| ui(f, state))?;
        }
    })
}

/// Builds the background writer when the tree has a vault to save to.
fn start_autosave(
    state: &TreeState,
    passphrase: Option<Zeroizing<Vec<u8>>>,
    events: mpsc::UnboundedSender<SaveEvent>,
) -> io::Result<Option<Autosave>> {
    let Some(path) = state.active_vault_path.clone() else {
        return Ok(None);
    };
    let Some(passphrase) = passphrase else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "autosave requires a vault passphrase",
        ));
    };

    let save = move |root: &Node| {
        save_tree(root, &path, &passphrase).map_err(|e| SaveError::Failed(e.to_string()))
    };
    let on_event = move |event| {
        let _ = events.send(event);
    };

    Autosave::spawn(save, on_event).map(Some)
}

/// Waits until the snapshot with `target` generation has been attempted.
async fn wait_for_save(
    save_rx: &mut mpsc::UnboundedReceiver<SaveEvent>,
    state: &mut TreeState,
    saved: &mut u64,
    target: u64,
) -> Result<(), SaveError> {
    loop {
        match save_rx.recv().await {
            Some(event) => {
                if event.result.is_ok() {
                    *saved = (*saved).max(event.generation);
                }
                if event.generation == target {
                    return match event.result {
                        Ok(()) => {
                            state.status = None;
                            Ok(())
                        }
                        Err(error) => Err(error),
                    };
                }
            }
            None => return Err(SaveError::WorkerStopped),
        }
    }
}

/// Dispatches a key event to the handler for the current input mode.
fn dispatch_key(key: event::KeyEvent, state: &mut TreeState) {
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
        InputMode::Detail { .. } => handle_detail(key, state),
        InputMode::ExportNodeSelect { .. } => handle_export_node_select(key, state),
        InputMode::ExportNodeFormat { .. } => handle_export_node_format(key, state),
        InputMode::Alert { .. } => handle_alert(key, state),
        InputMode::Help => handle_help(key, state),
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
            if state.cut_buffer.is_some() {
                state.mode = InputMode::PendingPaste;
            }
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
    /// Adds a Description extra field (`Custom("Description")`).
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

/// Applies an edit to the form's scratch text while at the Input level.
fn scratch_edit<F>(form: &mut FormState, edit: F)
where
    F: FnOnce(&mut String, &mut usize),
{
    if let Some(scratch) = form.scratch.as_mut() {
        edit(&mut scratch.text, &mut scratch.cursor);
    }
}

/// Records a committed change on the form's node.
///
/// On the Add form's first commit this inserts the draft into the tree at
/// [`TreeState::add_position`], switches the form to editing that node, and
/// seeds the undo history. In all cases the resulting node state is pushed onto
/// the history and the tree is marked dirty (triggering autosave).
fn commit_form_change(state: &mut TreeState, form: &mut FormState) {
    if form.is_adding() {
        let placeholder = Node::new(String::new());
        let mut draft = std::mem::replace(&mut form.draft, placeholder);
        if draft.name.is_empty() {
            draft.name = texts::DEFAULT_NAME.to_string();
        }
        match state.insert_node_at_position(state.add_position, draft) {
            Ok(id) => {
                form.node_id = Some(id);
                // The creating commit defines the baseline; it is not itself
                // undoable.
                form.seed_history(&state.root);
            }
            // The position is unavailable (e.g. a sibling slot for the root);
            // keep the draft so the user can keep editing.
            Err(node) => {
                form.draft = node;
                return;
            }
        }
    } else {
        form.push_history(&state.root);
    }
    state.modified = true;
}

/// Returns the mode to enter when the Edit/Add form is closed.
///
/// An Edit session (including an Add session whose first commit created its
/// node) returns to the node's Detail overlay; an empty Add session cancels
/// back to Normal.
fn close_form(form: &FormState) -> InputMode {
    if form.is_adding() {
        InputMode::Normal
    } else {
        InputMode::Detail {
            selected: 0,
            alert: None,
        }
    }
}

fn handle_form(key: event::KeyEvent, state: &mut TreeState) {
    let mut form = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::Form(form) => form,
        other => {
            state.mode = other;
            return;
        }
    };

    form.clamp_selection(&state.root);
    let row = form.current_row(&state.root);

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
            if form.cut_focused_field(&mut state.root) {
                commit_form_change(state, &mut form);
            }
            form.pending = FormPending::None;
        }
        Action::PasteStart => {
            if form.field_cut_buffer.is_some() && matches!(row, FormRow::Extra(_)) {
                form.pending = FormPending::Paste;
            }
        }
        Action::PasteSibling => {
            if form.paste_field_after(&mut state.root) {
                commit_form_change(state, &mut form);
            }
            form.pending = FormPending::None;
        }
        Action::PasteBefore => {
            if form.paste_field_before(&mut state.root) {
                commit_form_change(state, &mut form);
            }
            form.pending = FormPending::None;
        }
        Action::BeginInput if row == FormRow::AddButton => {
            state.mode = InputMode::SelectFieldType { form, selected: 0 };
            return;
        }
        Action::BeginInput => {
            form.begin_input(&state.root);
        }
        Action::ExitInput => {
            if form.commit_input(&mut state.root) {
                commit_form_change(state, &mut form);
            }
        }
        Action::Undo => {
            if form.undo(&mut state.root) {
                state.modified = true;
            }
        }
        Action::Redo => {
            if form.redo(&mut state.root) {
                state.modified = true;
            }
        }
        // Delete on a Name row or the add button has no meaning in Nav.
        Action::DeleteChar if row == FormRow::AddButton => {}
        Action::DeleteChar if row == FormRow::Name && form.focus == FormFocus::Nav => {}
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
            scratch_edit(&mut form, |text, cursor| delete_at(text, *cursor));
        }
        Action::Cancel => {
            state.mode = close_form(&form);
            return;
        }
        Action::NextField => {
            let len = form.rows(&state.root).len();
            let next = (form.selected_row + 1) % len;
            form.select_row(&state.root, next);
        }
        Action::PrevField => {
            let len = form.rows(&state.root).len();
            let prev = (form.selected_row + len - 1) % len;
            form.select_row(&state.root, prev);
        }
        Action::CursorLeft => {
            scratch_edit(&mut form, |text, cursor| {
                *cursor = cursor_left(text, *cursor)
            });
        }
        Action::CursorRight => {
            scratch_edit(&mut form, |text, cursor| {
                *cursor = cursor_right(text, *cursor)
            });
        }
        Action::CursorHome => {
            scratch_edit(&mut form, |_, cursor| *cursor = 0);
        }
        Action::CursorEnd => {
            scratch_edit(&mut form, |text, cursor| *cursor = text.len());
        }
        Action::Backspace => {
            scratch_edit(&mut form, |text, cursor| {
                *cursor = backspace_at(text, *cursor);
            });
        }
        Action::InsertChar(c) => {
            scratch_edit(&mut form, |text, cursor| insert_char_at(text, cursor, c));
        }
        _ => {}
    }

    state.mode = InputMode::Form(form);
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
                let index = form.push_field(
                    &mut state.root,
                    ExtraField {
                        field_type: FieldType::Login,
                        value: String::new(),
                    },
                );
                form.focus_new_field(&state.root, index);
                commit_form_change(state, &mut form);
                state.mode = InputMode::Form(form);
            }
            PickerItem::Password => {
                let index = form.push_field(
                    &mut state.root,
                    ExtraField {
                        field_type: FieldType::Password,
                        value: String::new(),
                    },
                );
                form.focus_new_field(&state.root, index);
                commit_form_change(state, &mut form);
                state.mode = InputMode::Form(form);
            }
            PickerItem::Custom => {
                state.mode = InputMode::CustomFieldName {
                    form,
                    input: String::new(),
                };
            }
            PickerItem::Desc => {
                let index =
                    form.push_field(&mut state.root, ExtraField::description(String::new()));
                form.focus_new_field(&state.root, index);
                commit_form_change(state, &mut form);
                state.mode = InputMode::Form(form);
            }
        },
        _ => {
            state.mode = InputMode::SelectFieldType { form, selected };
        }
    }
}

/// Items offered by the field-type picker for the given form.
fn picker_items(_form: &FormState) -> Vec<PickerItem> {
    vec![
        PickerItem::Login,
        PickerItem::Password,
        PickerItem::Custom,
        PickerItem::Desc,
    ]
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
            let index = form.push_field(
                &mut state.root,
                ExtraField {
                    field_type: FieldType::Custom(label),
                    value: String::new(),
                },
            );
            form.focus_new_field(&state.root, index);
            commit_form_change(state, &mut form);
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

    if Action::from_confirm(&key) == Action::Yes && form.remove_field(&mut state.root, target_index)
    {
        form.focus = FormFocus::Nav;
        form.clamp_selection(&state.root);
        form.cursor = form.active_len(&state.root);
        commit_form_change(state, &mut form);
    }
    state.mode = InputMode::Form(form);
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

    let field_count = state.current_node_ref().map_or(0, |n| n.extra_fields.len());

    match Action::from_detail(&key) {
        Action::Cancel => {
            state.mode = InputMode::Normal;
        }
        Action::NavDown => {
            let last = field_count.saturating_sub(1);
            let new_sel = (selected + 1).min(last);
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
        Action::NextField if field_count > 0 => {
            let new_sel = (selected + 1) % field_count;
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::PrevField if field_count > 0 => {
            let new_sel = (selected + field_count - 1) % field_count;
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::CopyField => {
            let text = state
                .current_node_ref()
                .and_then(|node| node.extra_fields.get(selected).map(|ef| ef.value.clone()));
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
            render_edit_overlay(frame, area, form, &state.root, title);
        }
        InputMode::SelectFieldType { .. } => render_select_field_type_overlay(frame, area, state),
        InputMode::CustomFieldName { .. } => render_custom_field_name_overlay(frame, area, state),
        InputMode::ConfirmDeleteField { .. } => render_confirm_delete_overlay(frame, area, state),
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

    if let Some(message) = state.save_error.as_deref() {
        render_save_error_overlay(frame, area, message);
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
        InputMode::Form(form) => form_hint(form, &state.root),
        InputMode::SelectFieldType { .. } => texts::HINT_SELECT_FIELD_TYPE,
        InputMode::CustomFieldName { .. } => texts::HINT_CUSTOM_FIELD_NAME,
        InputMode::ConfirmDeleteField { .. } => texts::HINT_CONFIRM_DELETE,
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

fn render_save_error_overlay(frame: &mut Frame, area: Rect, message: &str) {
    let width = (area.width * 2 / 3).max(1);
    let inner_width = width.saturating_sub(2).max(1) as usize;
    // Count the leading space rendered before the message.
    let message_rows = (message.chars().count() + 1).div_ceil(inner_width).max(1) as u16;
    let height = (message_rows + 4).min(area.height).max(1);

    let overlay_area = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SAVE_ERROR)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", message),
            ElementStyle::ConfirmMessage.style(),
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
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
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
    text: &str,
    cursor: usize,
    is_focused: bool,
    is_editing: bool,
) -> Line<'static> {
    let revealed = is_focused && is_editing;
    let display_val = if revealed {
        Cow::Borrowed(text)
    } else {
        ef.field_type.display_value(text)
    };
    let label = format!("{}: ", ef.field_type.label());
    form_row_line(&label, &display_val, cursor, is_focused, is_editing)
}

/// Resolves the text and cursor to render for a field row.
///
/// The focused row at the Input level shows the scratch buffer; every other
/// row shows the node's stored value.
fn field_display<'a>(form: &'a FormState, value: &'a str, focused: bool) -> (Cow<'a, str>, usize) {
    match form
        .scratch
        .as_ref()
        .filter(|_| focused && form.focus == FormFocus::Input)
    {
        Some(scratch) => (Cow::Borrowed(scratch.text.as_str()), scratch.cursor),
        None => (Cow::Borrowed(value), form.cursor),
    }
}

fn render_edit_overlay(frame: &mut Frame, area: Rect, form: &FormState, root: &Node, title: &str) {
    let node = form.node(root);
    let rows = form.rows(root);
    let current = form.selected_row.min(rows.len() - 1);
    let is_editing = form.focus == FormFocus::Input;

    let overlay_area = make_overlay_area(area, node.extra_fields.len(), 5);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .style(ElementStyle::EditBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        let focused = i == current;
        match row {
            FormRow::Name => {
                let (text, cursor) = field_display(form, &node.name, focused);
                lines.push(form_row_line(
                    texts::LABEL_NAME,
                    &text,
                    cursor,
                    focused,
                    is_editing,
                ));
            }
            FormRow::Extra(idx) => {
                if let Some(ef) = node.extra_fields.get(*idx) {
                    let (text, cursor) = field_display(form, &ef.value, focused);
                    lines.push(render_extra_field_line(
                        ef, &text, cursor, focused, is_editing,
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
        form_hint(form, root),
        ElementStyle::GrayHint.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Selects the help-bar hint for the Add/Edit form.
fn form_hint(form: &FormState, root: &Node) -> &'static str {
    match form.pending {
        FormPending::Cut => return texts::HINT_FIELD_CUT,
        FormPending::Paste => return texts::HINT_FIELD_PASTE,
        FormPending::None => {}
    }
    if form.focus == FormFocus::Input {
        return texts::HINT_FORM_INPUT;
    }
    match form.current_row(root) {
        FormRow::AddButton => texts::HINT_FORM_ADD,
        FormRow::Extra(_) => texts::HINT_FORM_EXTRA,
        FormRow::Name => texts::HINT_FORM,
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
            .node(&state.root)
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

    for (i, ef) in node.extra_fields.iter().enumerate() {
        let display_val = ef.field_type.display_value(&ef.value);
        let is_sel = selected == i;
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
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn editing_state() -> TreeState {
        let root = Node::new("Root");
        let mut state = TreeState::new(root);
        let node = state.root.clone();
        state.mode = InputMode::Form(FormState::new_edit(&node));
        state
    }

    fn editing_state_with_fields() -> TreeState {
        let mut node = Node::new("Root");
        for value in ["a", "b", "c"] {
            node.extra_fields.push(ExtraField {
                field_type: FieldType::Login,
                value: value.to_string(),
            });
        }
        let mut state = TreeState::new(node);
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

    fn form_node(state: &TreeState) -> &Node {
        form_of(state).node(&state.root)
    }

    fn values(state: &TreeState) -> Vec<&str> {
        form_node(state)
            .extra_fields
            .iter()
            .map(|f| f.value.as_str())
            .collect()
    }

    fn select_row(state: &mut TreeState, index: usize) {
        let InputMode::Form(form) = &mut state.mode else {
            panic!("expected Form mode");
        };
        form.select_row(&state.root, index);
    }

    fn type_text(state: &mut TreeState, text: &str) {
        for c in text.chars() {
            handle_form(key(KeyCode::Char(c)), state);
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
        assert!(matches!(state.mode, InputMode::Detail { .. }));
    }

    #[test]
    fn empty_add_form_esc_cancels_to_normal() {
        let mut state = TreeState::new(Node::new("Root"));
        state.mode = InputMode::Form(FormState::new_add());
        handle_form(key(KeyCode::Esc), &mut state);
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
        assert!(matches!(state.mode, InputMode::Form(_)));
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
        state.mode = InputMode::Form(FormState::new_add());
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
        state.mode = InputMode::Form(FormState::new_add());
        select_row(&mut state, 1); // the + Add Field row

        handle_form(key(KeyCode::Enter), &mut state);
        assert!(matches!(state.mode, InputMode::SelectFieldType { .. }));

        // Description is offered last.
        let items = match &state.mode {
            InputMode::SelectFieldType { form, .. } => picker_items(form),
            _ => unreachable!(),
        };
        assert_eq!(items.last(), Some(&PickerItem::Desc));

        let desc_index = items.len() - 1;
        if let InputMode::SelectFieldType {
            ref mut selected, ..
        } = state.mode
        {
            *selected = desc_index;
        }
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
        assert!(matches!(state.mode, InputMode::Form(_)));
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
        state.mode = match std::mem::replace(&mut state.mode, InputMode::Normal) {
            InputMode::Form(form) => InputMode::ConfirmDeleteField {
                form,
                target_index: 1,
            },
            _ => unreachable!(),
        };
        handle_confirm_delete(key(KeyCode::Char('y')), &mut state);
        assert_eq!(values(&state), vec!["a", "c"]);
        assert!(form_of(&state).selected_row < form_of(&state).rows(&state.root).len());
    }

    /// A tree whose root has three extra fields, opened in the Detail overlay.
    fn detail_state() -> TreeState {
        let mut node = Node::new("Root");
        for value in ["a", "b", "c"] {
            node.extra_fields.push(ExtraField {
                field_type: FieldType::Login,
                value: value.to_string(),
            });
        }
        let mut state = TreeState::new(node);
        state.mode = InputMode::Detail {
            selected: 0,
            alert: None,
        };
        state
    }

    fn detail_selected(state: &TreeState) -> usize {
        match &state.mode {
            InputMode::Detail { selected, .. } => *selected,
            other => panic!("expected Detail mode, got {other:?}"),
        }
    }

    #[test]
    fn detail_tab_wraps_forward_and_backtab_wraps_backward() {
        let mut state = detail_state();
        handle_detail(key(KeyCode::Tab), &mut state);
        assert_eq!(detail_selected(&state), 1);
        handle_detail(key(KeyCode::Tab), &mut state);
        handle_detail(key(KeyCode::Tab), &mut state);
        assert_eq!(detail_selected(&state), 0); // wrapped past the last field

        handle_detail(key(KeyCode::BackTab), &mut state);
        assert_eq!(detail_selected(&state), 2); // wrapped past the first field
    }

    #[test]
    fn detail_nav_down_clamps_at_last_field() {
        let mut state = detail_state();
        for _ in 0..5 {
            handle_detail(key(KeyCode::Char('j')), &mut state);
        }
        assert_eq!(detail_selected(&state), 2); // last field, no phantom row
    }

    #[test]
    fn detail_nav_up_clamps_at_first_field() {
        let mut state = detail_state();
        handle_detail(key(KeyCode::Char('k')), &mut state);
        assert_eq!(detail_selected(&state), 0);
    }

    #[test]
    fn detail_tab_is_safe_without_fields() {
        let mut state = TreeState::new(Node::new("Root"));
        state.mode = InputMode::Detail {
            selected: 0,
            alert: None,
        };
        handle_detail(key(KeyCode::Tab), &mut state);
        handle_detail(key(KeyCode::BackTab), &mut state);
        assert_eq!(detail_selected(&state), 0);
    }

    #[test]
    fn detail_y_sets_copy_alert() {
        // An empty value skips the clipboard subprocess but still alerts.
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: String::new(),
        });
        let mut state = TreeState::new(node);
        state.mode = InputMode::Detail {
            selected: 0,
            alert: None,
        };
        handle_detail(key(KeyCode::Char('y')), &mut state);
        match &state.mode {
            InputMode::Detail { alert, .. } => {
                assert_eq!(alert.as_deref(), Some(texts::COPY_CHECK));
            }
            other => panic!("expected Detail mode, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn wait_for_save_reports_the_target_generation() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut state = TreeState::new(Node::new("Root"));
        state.status = Some(texts::STATUS_SAVING);
        let mut saved = 0;

        tx.send(SaveEvent {
            generation: 1,
            result: Ok(()),
        })
        .unwrap();
        tx.send(SaveEvent {
            generation: 3,
            result: Ok(()),
        })
        .unwrap();

        let result = wait_for_save(&mut rx, &mut state, &mut saved, 3).await;

        assert_eq!(result, Ok(()));
        assert_eq!(saved, 3);
        assert_eq!(state.status, None);
    }

    #[tokio::test]
    async fn wait_for_save_keeps_the_last_successful_generation_on_error() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut state = TreeState::new(Node::new("Root"));
        let mut saved = 0;

        tx.send(SaveEvent {
            generation: 1,
            result: Ok(()),
        })
        .unwrap();
        tx.send(SaveEvent {
            generation: 2,
            result: Err(SaveError::Failed("disk full".to_string())),
        })
        .unwrap();

        let result = wait_for_save(&mut rx, &mut state, &mut saved, 2).await;

        assert_eq!(result, Err(SaveError::Failed("disk full".to_string())));
        assert_eq!(saved, 1);
    }

    #[tokio::test]
    async fn wait_for_save_detects_a_stopped_worker() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut state = TreeState::new(Node::new("Root"));
        let mut saved = 0;

        let result = wait_for_save(&mut rx, &mut state, &mut saved, 1).await;

        assert_eq!(result, Err(SaveError::WorkerStopped));
    }
}
