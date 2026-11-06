//! The Detail modal's Add/Edit Form screen.
//!
//! The form live-edits a Node's name and extra fields: navigation, the Input
//! level with its scratch buffer, the field cut/paste prefixes, undo/redo, and
//! the commit helpers that push a change onto the history and mark the tree
//! dirty (triggering autosave). It also owns the field-generator dispatch
//! (`Ctrl+G`), which either rerolls a login word in place or opens the password
//! generator (see [`super::generator`]).

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::super::chrome::make_overlay_area;
use super::{Detail, DetailHit, DetailPopup, GENERATOR_ROW_LENGTH, TreeAction, plain_key};
use crate::cursor::{
    backspace_at, cursor_field_spans, cursor_left, cursor_right, delete_at, insert_char_at,
};
use crate::generator;
use crate::hitbox::{ClickSurface, RowClick, record_rows};
use crate::theming::{Action, ElementStyle, Icon, texts};
use crate::tree::node::{ExtraField, FieldType, Node};
use crate::tree::widget::{FormFocus, FormPending, FormRow, FormState, GeneratorModal, TreeState};

impl Detail {
    /// Keys while the Add/Edit form is open.
    pub(super) fn handle_form(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let mut form = self.form.take().expect("the form is open");
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
            self.form = Some(form);
            return None;
        }

        match action {
            Action::Generate => {
                // `Ctrl+G` is bound at the Input level and only when the focused
                // row carries a generator (a Login or Password field). Other rows
                // fall through and the form is restored untouched.
                match FieldGenerator::for_field(form.focused_field_type(&state.root)) {
                    Some(FieldGenerator::Password) => {
                        let length_input = state.generator.length.value().to_string();
                        let length_cursor = length_input.len();
                        self.popup = DetailPopup::GeneratePassword(GeneratorModal {
                            params: state.generator,
                            length_input,
                            length_cursor,
                            length_fresh: true,
                            error: None,
                            selected: GENERATOR_ROW_LENGTH,
                            form,
                        });
                        return None;
                    }
                    Some(FieldGenerator::Login) => {
                        // A login word has no parameters, so it is generated on
                        // the spot: no modal, and `Ctrl+G` again rerolls.
                        let value = generator::generate_login();
                        write_generated_value(state, &mut form, value);
                        self.form = Some(form);
                        return None;
                    }
                    None => {}
                }
            }
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
                self.popup = DetailPopup::SelectFieldType { selected: 0 };
                self.form = Some(form);
                return None;
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
                    // Undo can restore a different name; refresh the visible list.
                    state.update_visible();
                }
            }
            Action::Redo => {
                if form.redo(&mut state.root) {
                    state.modified = true;
                    // Redo can restore a different name; refresh the visible list.
                    state.update_visible();
                }
            }
            // Delete on a Name row or the add button has no meaning in Nav.
            Action::DeleteChar if row == FormRow::AddButton => {}
            Action::DeleteChar if row == FormRow::Name && form.focus == FormFocus::Nav => {}
            Action::DeleteChar => {
                if let FormRow::Extra(i) = row
                    && form.focus == FormFocus::Nav
                {
                    self.popup = DetailPopup::ConfirmDeleteField { target_index: i };
                    self.form = Some(form);
                    return None;
                }
                scratch_edit(&mut form, |text, cursor| delete_at(text, *cursor));
            }
            Action::Cancel => {
                // An empty Add session cancels the whole modal; an Edit session
                // (including a created Add) drops back to browsing.
                if form.is_adding() {
                    return Some(TreeAction::Close);
                }
                return None;
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

        self.form = Some(form);
        None
    }

    /// A click on a form row: select it, or on a repeat enter it.
    pub(super) fn click_form_row(
        &mut self,
        index: usize,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let form = self.form.as_mut()?;
        let click = RowClick::resolve(form.selected_row, index);
        form.select_row(&state.root, index);
        if click.activated() {
            self.handle_key(plain_key(KeyCode::Enter), state)
        } else {
            None
        }
    }

    /// Draws the Add/Edit form.
    pub(super) fn render_form(&mut self, frame: &mut Frame, area: Rect, state: &TreeState) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let title = if form.is_adding() {
            texts::OVERLAY_ADD_NODE
        } else {
            texts::OVERLAY_EDIT_NODE
        };
        let panel = render_edit_overlay(frame, area, form, &state.root, title, &mut self.clicks);
        self.clicks.set_panel(panel);
    }
}

/// Which generator a field type has, if any.
///
/// An enum rather than a trait: a new variant forces every `match` to be
/// revisited. It is the single source of truth for both the key handler and the
/// form hint. It carries no parameters — those differ per field type (a login
/// word has none, a password has [`PasswordParams`](crate::generator::PasswordParams)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldGenerator {
    Login,
    Password,
}

impl FieldGenerator {
    /// The generator for `field_type`, or `None` when it has none.
    pub(super) fn for_field(field_type: Option<&FieldType>) -> Option<Self> {
        match field_type {
            Some(FieldType::Login) => Some(Self::Login),
            Some(FieldType::Password) => Some(Self::Password),
            _ => None,
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
pub(super) fn commit_form_change(state: &mut TreeState, form: &mut FormState) {
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
        // A commit can rename the node, but the flat visible list caches names.
        // Rebuild it so the tree shows the new name. (The Add path above already
        // refreshes visibility via `insert_node_at_position`.)
        state.update_visible();
    }
    state.modified = true;
}

/// Records a generation as a committed change.
///
/// Like [`commit_form_change`] it marks the tree modified (triggering autosave);
/// unlike it, consecutive generations coalesce into a single undo step. An Add
/// form's first commit still creates its node and seeds the baseline.
fn commit_generation(state: &mut TreeState, form: &mut FormState) {
    if form.is_adding() {
        commit_form_change(state, form);
        return;
    }
    form.push_history_coalescing(&state.root);
    state.modified = true;
}

/// Writes a generated `value` into the form's focused field.
///
/// Sets the field, keeps the Input-level scratch buffer in step (so a revealed
/// field shows the value immediately and leaving Input does not restore the old
/// value), places the cursor at the end, and records a coalescing commit. Leaves
/// the value untouched and returns `false` when the focused row is not an extra
/// field.
pub(super) fn write_generated_value(
    state: &mut TreeState,
    form: &mut FormState,
    value: String,
) -> bool {
    if !form.set_focused_value(&mut state.root, value.clone()) {
        return false;
    }
    // The generator is invoked from the Input level, so the focused text lives
    // in the scratch buffer.
    if form.focus == FormFocus::Input
        && let Some(scratch) = form.scratch.as_mut()
    {
        scratch.text = value;
        scratch.cursor = scratch.text.len();
    }
    form.cursor = form.active_len(&state.root);
    commit_generation(state, form);
    true
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
        spans.extend(cursor_field_spans(label, text, cursor));
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
pub(crate) fn render_extra_field_line(
    ef: &ExtraField,
    text: &str,
    cursor: usize,
    is_focused: bool,
    is_editing: bool,
) -> Line<'static> {
    let revealed = is_focused && is_editing;
    let display_val = if revealed {
        std::borrow::Cow::Borrowed(text)
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
fn field_display<'a>(
    form: &'a FormState,
    value: &'a str,
    focused: bool,
) -> (std::borrow::Cow<'a, str>, usize) {
    match form
        .scratch
        .as_ref()
        .filter(|_| focused && form.focus == FormFocus::Input)
    {
        Some(scratch) => (
            std::borrow::Cow::Borrowed(scratch.text.as_str()),
            scratch.cursor,
        ),
        None => (std::borrow::Cow::Borrowed(value), form.cursor),
    }
}

/// Renders the Add/Edit form overlay.
fn render_edit_overlay(
    frame: &mut Frame,
    area: Rect,
    form: &FormState,
    root: &Node,
    title: &str,
    clicks: &mut ClickSurface<DetailHit>,
) -> Rect {
    let node = form.node(root);
    let rows = form.rows(root);
    let current = form.selected_row.min(rows.len() - 1);
    let is_editing = form.focus == FormFocus::Input;

    let overlay_area = make_overlay_area(area, node.extra_fields.len(), 4);

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

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);

    // Every form row is one line tall, starting at the overlay's first inner
    // row. Only rows the card actually shows can be clicked.
    let visible = rows.len().min(inner.height as usize);
    record_rows(clicks, inner, 0, visible, DetailHit::FormRow);

    overlay_area
}

/// Selects the help-bar hint for the Add/Edit form.
pub(crate) fn form_hint(form: &FormState, root: &Node) -> &'static str {
    match form.pending {
        FormPending::Cut => return texts::HINT_FIELD_CUT,
        FormPending::Paste => return texts::HINT_FIELD_PASTE,
        FormPending::None => {}
    }
    if form.focus == FormFocus::Input {
        return if FieldGenerator::for_field(form.focused_field_type(root)).is_some() {
            texts::HINT_FORM_GENERATOR_INPUT
        } else {
            texts::HINT_FORM_INPUT
        };
    }
    match form.current_row(root) {
        FormRow::AddButton => texts::HINT_FORM_ADD,
        FormRow::Extra(_) => {
            if form.field_cut_buffer.is_some() {
                texts::HINT_FORM_EXTRA_PASTE
            } else {
                texts::HINT_FORM_EXTRA
            }
        }
        FormRow::Name => texts::HINT_FORM,
    }
}
