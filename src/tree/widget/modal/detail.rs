//! The Detail modal: a selected Node's ExtraFields and the Form Popup that edits
//! it.
//!
//! Browsing can copy a value to the system clipboard (reported as
//! [`TreeAction::CopyToClipboard`]) and open the Form. The Form is a Popup with
//! nested Popups of its own (the field-type picker, the custom-name prompt, the
//! delete confirmation, and the password generator with its symbol checklist).
//! Because the Form live-edits a Node, the modal takes the [`TreeState`] to do
//! that in-memory work; every effect outside the process is still a
//! [`TreeAction`].

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use super::chrome::{centered_rect, make_overlay_area};
use crate::cursor::{
    backspace_at, cursor_field_spans, cursor_left, cursor_right, delete_at, insert_char_at,
    selected_span,
};
use crate::generator::{self, PasswordLength, PasswordParams};
use crate::theming::{Action, ElementStyle, Icon, texts};
use crate::tree::node::{ExtraField, FieldType, Node};
use crate::tree::widget::{FormFocus, FormPending, FormRow, FormState, GeneratorModal, TreeState};

/// Rows of the password-generator modal, in display order.
pub(crate) const GENERATOR_ROW_LENGTH: usize = 0;
pub(crate) const GENERATOR_ROW_SYMBOLS: usize = 1;
pub(crate) const GENERATOR_ROW_GENERATE: usize = 2;
const GENERATOR_ROWS: usize = 3;

/// Columns in the symbol checklist grid.
const SYMBOL_COLUMNS: usize = 3;

/// The Detail modal: browse a selected Node's ExtraFields, copy a value, or edit.
#[derive(Debug, Default)]
pub(crate) struct Detail {
    /// Highlighted extra-field row while browsing.
    selected: usize,
    /// Copy-confirmation alert text, if any. The next key dismisses it.
    alert: Option<String>,
    /// The open Form Popup, or `None` while browsing. Held inside the generator
    /// popups rather than here while one of those is up.
    form: Option<FormState>,
    /// The Popup nested over the Form, if any.
    popup: DetailPopup,
}

/// A Popup nested over the Detail modal.
#[derive(Debug, Default)]
enum DetailPopup {
    /// No Popup: browsing, or editing in the Form itself.
    #[default]
    None,
    /// The field-type picker (`+ Add Field`).
    SelectFieldType { selected: usize },
    /// Prompt for a new Custom field's name.
    CustomFieldName { input: String },
    /// The parameter modal of the password generator (`Ctrl+G` on a Password).
    GeneratePassword(GeneratorModal),
    /// The per-symbol checklist opened from the generator modal.
    SymbolChecklist(GeneratorModal),
    /// Confirmation for deleting an extra field.
    ConfirmDeleteField { target_index: usize },
}

/// Which screen the modal is showing, derived from its state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Browse,
    Form,
    SelectFieldType,
    CustomFieldName,
    GeneratePassword,
    SymbolChecklist,
    ConfirmDeleteField,
}

impl Detail {
    /// A modal browsing the selected Node, with no Form open.
    pub(crate) fn browse() -> Self {
        Self::default()
    }

    /// A modal editing `node` in its Form Popup.
    #[cfg(test)]
    pub(crate) fn editing(node: &Node) -> Self {
        Self {
            selected: 0,
            alert: None,
            form: Some(FormState::new_edit(node)),
            popup: DetailPopup::None,
        }
    }

    /// A modal creating a new Node in its Form Popup.
    pub(crate) fn adding() -> Self {
        Self {
            selected: 0,
            alert: None,
            form: Some(FormState::new_add()),
            popup: DetailPopup::None,
        }
    }

    /// Reads a key, updating the state and reporting what the user asked for.
    ///
    /// Only effects outside the process become actions: [`TreeAction::Close`] to
    /// dismiss the modal, [`TreeAction::CopyToClipboard`] on a copy.
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        match self.screen() {
            Screen::Browse => self.handle_browse(key, state),
            Screen::Form => self.handle_form(key, state),
            Screen::SelectFieldType => self.handle_select_field_type(key, state),
            Screen::CustomFieldName => self.handle_custom_field_name(key, state),
            Screen::GeneratePassword => self.handle_generate_password(key, state),
            Screen::SymbolChecklist => self.handle_symbol_checklist(key, state),
            Screen::ConfirmDeleteField => self.handle_confirm_delete(key, state),
        }
    }

    /// The screen implied by the current state.
    fn screen(&self) -> Screen {
        match &self.popup {
            DetailPopup::None if self.form.is_some() => Screen::Form,
            DetailPopup::None => Screen::Browse,
            DetailPopup::SelectFieldType { .. } => Screen::SelectFieldType,
            DetailPopup::CustomFieldName { .. } => Screen::CustomFieldName,
            DetailPopup::GeneratePassword(_) => Screen::GeneratePassword,
            DetailPopup::SymbolChecklist(_) => Screen::SymbolChecklist,
            DetailPopup::ConfirmDeleteField { .. } => Screen::ConfirmDeleteField,
        }
    }

    /// Keys while browsing the Node's extra fields.
    fn handle_browse(&mut self, key: KeyEvent, state: &mut TreeState) -> Option<TreeAction> {
        // A copy alert consumes the next key and clears itself.
        if self.alert.is_some() {
            self.alert = None;
            return None;
        }

        let field_count = state.current_node_ref().map_or(0, |n| n.extra_fields.len());

        match Action::from_detail(&key) {
            Action::Cancel => Some(TreeAction::Close),
            Action::NavDown => {
                self.selected = (self.selected + 1).min(field_count.saturating_sub(1));
                None
            }
            Action::NavUp => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            Action::NextField if field_count > 0 => {
                self.selected = (self.selected + 1) % field_count;
                None
            }
            Action::PrevField if field_count > 0 => {
                self.selected = (self.selected + field_count - 1) % field_count;
                None
            }
            Action::CopyField => {
                // With nothing selected (a node that has no extra fields) `y` is
                // a no-op: never claim success without a value to copy.
                let text = state.current_node_ref().and_then(|node| {
                    node.extra_fields
                        .get(self.selected)
                        .map(|ef| ef.value.clone())
                })?;
                self.alert = Some(texts::COPY_CHECK.to_string());
                if text.is_empty() {
                    None
                } else {
                    Some(TreeAction::CopyToClipboard { text })
                }
            }
            Action::EditNode => {
                if let Some(node) = state.current_node_ref() {
                    self.form = Some(FormState::new_edit(node));
                }
                None
            }
            _ => None,
        }
    }

    /// Keys while the Add/Edit form is open.
    fn handle_form(&mut self, key: KeyEvent, state: &mut TreeState) -> Option<TreeAction> {
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

    /// Keys for the field-type picker opened from the `+ Add Field` row.
    fn handle_select_field_type(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let selected = match std::mem::take(&mut self.popup) {
            DetailPopup::SelectFieldType { selected } => selected,
            other => {
                self.popup = other;
                return None;
            }
        };
        let mut form = self.form.take().expect("the form is open");
        let items = picker_items();
        let selected = selected.min(items.len().saturating_sub(1));

        match Action::from_select_field_type(&key) {
            Action::Cancel => {
                self.form = Some(form);
            }
            Action::NavDown => {
                self.popup = DetailPopup::SelectFieldType {
                    selected: (selected + 1) % items.len(),
                };
                self.form = Some(form);
            }
            Action::NavUp => {
                self.popup = DetailPopup::SelectFieldType {
                    selected: (selected + items.len() - 1) % items.len(),
                };
                self.form = Some(form);
            }
            Action::SelectIndex(i) => {
                let selected = if i < items.len() { i } else { selected };
                self.popup = DetailPopup::SelectFieldType { selected };
                self.form = Some(form);
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
                    self.form = Some(form);
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
                    self.form = Some(form);
                }
                PickerItem::Custom => {
                    self.popup = DetailPopup::CustomFieldName {
                        input: String::new(),
                    };
                    self.form = Some(form);
                }
                PickerItem::Desc => {
                    let index =
                        form.push_field(&mut state.root, ExtraField::description(String::new()));
                    form.focus_new_field(&state.root, index);
                    commit_form_change(state, &mut form);
                    self.form = Some(form);
                }
            },
            _ => {
                self.popup = DetailPopup::SelectFieldType { selected };
                self.form = Some(form);
            }
        }
        None
    }

    /// Keys for the Custom field-name prompt.
    fn handle_custom_field_name(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let input = match std::mem::take(&mut self.popup) {
            DetailPopup::CustomFieldName { input } => input,
            other => {
                self.popup = other;
                return None;
            }
        };
        let mut form = self.form.take().expect("the form is open");

        match Action::from_custom_field_name(&key) {
            Action::Cancel => {
                let custom = picker_items()
                    .iter()
                    .position(|item| *item == PickerItem::Custom)
                    .unwrap_or(0);
                self.popup = DetailPopup::SelectFieldType { selected: custom };
                self.form = Some(form);
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
                self.form = Some(form);
            }
            Action::Backspace => {
                let mut input = input;
                input.pop();
                self.popup = DetailPopup::CustomFieldName { input };
                self.form = Some(form);
            }
            Action::InsertChar(c) => {
                let mut input = input;
                input.push(c);
                self.popup = DetailPopup::CustomFieldName { input };
                self.form = Some(form);
            }
            _ => {
                self.popup = DetailPopup::CustomFieldName { input };
                self.form = Some(form);
            }
        }
        None
    }

    /// Keys for the password-generator parameter modal.
    ///
    /// `Ctrl+G` opened this from the Add/Edit form's Input level. On the length
    /// row, the first digit replaces the seeded value and the caret can then be
    /// moved with `Left`/`Right`/`Home`/`End`, inserting/removing at most three
    /// digits. Generating writes the value into the focused field (and its
    /// scratch buffer), commits it (undo entry + autosave) and returns to the
    /// form still editing the field. `Esc` returns without touching the field.
    fn handle_generate_password(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let mut modal = match std::mem::take(&mut self.popup) {
            DetailPopup::GeneratePassword(modal) => modal,
            other => {
                self.popup = other;
                return None;
            }
        };

        match Action::from_generator(&key) {
            Action::Cancel => {
                self.form = Some(modal.form);
            }
            Action::NavDown => {
                modal.selected = (modal.selected + 1) % GENERATOR_ROWS;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::NavUp => {
                modal.selected = (modal.selected + GENERATOR_ROWS - 1) % GENERATOR_ROWS;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::Toggle => {
                if modal.selected == GENERATOR_ROW_SYMBOLS {
                    modal.params.symbols.toggle_all();
                }
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::CursorLeft if modal.selected == GENERATOR_ROW_LENGTH => {
                modal.length_fresh = false;
                modal.length_cursor = cursor_left(&modal.length_input, modal.length_cursor);
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::CursorRight if modal.selected == GENERATOR_ROW_LENGTH => {
                modal.length_fresh = false;
                modal.length_cursor = cursor_right(&modal.length_input, modal.length_cursor);
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::CursorHome if modal.selected == GENERATOR_ROW_LENGTH => {
                modal.length_fresh = false;
                modal.length_cursor = 0;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::CursorEnd if modal.selected == GENERATOR_ROW_LENGTH => {
                modal.length_fresh = false;
                modal.length_cursor = modal.length_input.len();
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::InsertChar(c)
                if modal.selected == GENERATOR_ROW_LENGTH && c.is_ascii_digit() =>
            {
                if modal.length_fresh {
                    modal.length_input.clear();
                    modal.length_cursor = 0;
                    modal.length_fresh = false;
                }
                if modal.length_input.len() < 3 {
                    insert_char_at(&mut modal.length_input, &mut modal.length_cursor, c);
                }
                modal.error = None;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::Backspace if modal.selected == GENERATOR_ROW_LENGTH => {
                if modal.length_fresh {
                    modal.length_input.clear();
                    modal.length_cursor = 0;
                    modal.length_fresh = false;
                } else {
                    modal.length_cursor =
                        backspace_at(&mut modal.length_input, modal.length_cursor);
                }
                modal.error = None;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::DeleteChar if modal.selected == GENERATOR_ROW_LENGTH => {
                if modal.length_fresh {
                    modal.length_input.clear();
                    modal.length_cursor = 0;
                    modal.length_fresh = false;
                } else {
                    delete_at(&mut modal.length_input, modal.length_cursor);
                }
                modal.error = None;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::Confirm if modal.selected == GENERATOR_ROW_LENGTH => {
                modal.selected = GENERATOR_ROW_SYMBOLS;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::Confirm if modal.selected == GENERATOR_ROW_SYMBOLS => {
                modal.selected = 0;
                self.popup = DetailPopup::SymbolChecklist(modal);
            }
            Action::Confirm if modal.selected == GENERATOR_ROW_GENERATE => {
                match modal.length_input.parse::<PasswordLength>() {
                    Ok(length) => {
                        let params = PasswordParams {
                            length,
                            symbols: modal.params.symbols,
                        };
                        let Some(FieldGenerator::Password) =
                            FieldGenerator::for_field(modal.form.focused_field_type(&state.root))
                        else {
                            // The row lost its generator (defensive); leave the
                            // value untouched and return to the form.
                            self.form = Some(modal.form);
                            return None;
                        };
                        let value = generator::generate_password(&params);
                        write_generated_value(state, &mut modal.form, value);
                        state.generator = params;
                        self.form = Some(modal.form);
                    }
                    Err(error) => {
                        modal.error = Some(error);
                        self.popup = DetailPopup::GeneratePassword(modal);
                    }
                }
            }
            _ => {
                self.popup = DetailPopup::GeneratePassword(modal);
            }
        }
        None
    }

    /// Keys for the per-symbol checklist opened from the generator modal.
    ///
    /// The symbols are laid out in a [`SYMBOL_COLUMNS`]-wide grid; `h`/`l` move
    /// one cell and `j`/`k` move one row. `Space`/`Enter` toggle the focused
    /// symbol, `Esc` returns to the parameter modal focused on the summary row.
    fn handle_symbol_checklist(
        &mut self,
        key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        let mut modal = match std::mem::take(&mut self.popup) {
            DetailPopup::SymbolChecklist(modal) => modal,
            other => {
                self.popup = other;
                return None;
            }
        };

        let last = generator::SYMBOLS.len() - 1;
        match Action::from_symbol_checklist(&key) {
            Action::Cancel => {
                modal.selected = GENERATOR_ROW_SYMBOLS;
                self.popup = DetailPopup::GeneratePassword(modal);
            }
            Action::NavLeft => {
                modal.selected = modal.selected.saturating_sub(1);
                self.popup = DetailPopup::SymbolChecklist(modal);
            }
            Action::NavRight => {
                modal.selected = (modal.selected + 1).min(last);
                self.popup = DetailPopup::SymbolChecklist(modal);
            }
            Action::NavUp => {
                modal.selected = modal.selected.saturating_sub(SYMBOL_COLUMNS);
                self.popup = DetailPopup::SymbolChecklist(modal);
            }
            Action::NavDown => {
                modal.selected = (modal.selected + SYMBOL_COLUMNS).min(last);
                self.popup = DetailPopup::SymbolChecklist(modal);
            }
            Action::Toggle => {
                if let Some(&symbol) = generator::SYMBOLS.get(modal.selected) {
                    modal.params.symbols.toggle(symbol);
                }
                self.popup = DetailPopup::SymbolChecklist(modal);
            }
            _ => {
                self.popup = DetailPopup::SymbolChecklist(modal);
            }
        }
        None
    }

    /// Keys for the delete-field confirmation popup.
    fn handle_confirm_delete(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let target_index = match std::mem::take(&mut self.popup) {
            DetailPopup::ConfirmDeleteField { target_index } => target_index,
            other => {
                self.popup = other;
                return None;
            }
        };
        let mut form = self.form.take().expect("the form is open");

        if Action::from_confirm(&key) == Action::Yes
            && form.remove_field(&mut state.root, target_index)
        {
            form.focus = FormFocus::Nav;
            form.clamp_selection(&state.root);
            form.cursor = form.active_len(&state.root);
            commit_form_change(state, &mut form);
        }
        self.form = Some(form);
        None
    }

    /// Draws the active screen over `area`.
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect, state: &TreeState) {
        match self.screen() {
            Screen::Browse => self.render_browse(frame, area, state),
            Screen::Form => self.render_form(frame, area, state),
            Screen::SelectFieldType => self.render_select_field_type(frame, area),
            Screen::CustomFieldName => self.render_custom_field_name(frame, area),
            Screen::GeneratePassword => {
                if let DetailPopup::GeneratePassword(modal) = &self.popup {
                    render_generate_password_overlay(frame, area, modal);
                }
            }
            Screen::SymbolChecklist => {
                if let DetailPopup::SymbolChecklist(modal) = &self.popup {
                    render_symbol_checklist_overlay(frame, area, modal);
                }
            }
            Screen::ConfirmDeleteField => self.render_confirm_delete(frame, area, state),
        }
    }

    /// Draws the browsed Node's ExtraFields, and the copy alert if it is armed.
    fn render_browse(&self, frame: &mut Frame, area: Rect, state: &TreeState) {
        let Some(node) = state.current_node_ref() else {
            return;
        };

        let overlay_area = make_overlay_area(area, node.extra_fields.len(), 3);

        let block = Block::default()
            .title(format!(" {} ", node.name))
            .borders(Borders::ALL)
            .style(ElementStyle::DetailBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(""));

        for (i, ef) in node.extra_fields.iter().enumerate() {
            let display_val = ef.field_type.display_value(&ef.value);
            let text = format!("{}: {}", ef.field_type.label(), display_val);
            lines.push(Line::from(selected_span(&text, self.selected == i)));
        }

        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);

        if self.alert.is_some() {
            render_copy_alert(frame, area);
        }
    }

    /// Draws the Add/Edit form.
    fn render_form(&self, frame: &mut Frame, area: Rect, state: &TreeState) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let title = if form.is_adding() {
            texts::OVERLAY_ADD_NODE
        } else {
            texts::OVERLAY_EDIT_NODE
        };
        render_edit_overlay(frame, area, form, &state.root, title);
    }

    /// Draws the field-type picker.
    fn render_select_field_type(&self, frame: &mut Frame, area: Rect) {
        if self.form.is_none() {
            return;
        }
        let DetailPopup::SelectFieldType { selected } = self.popup else {
            return;
        };
        let items = picker_items();

        let overlay_area = centered_rect(area, area.width / 2, 8);

        let block = Block::default()
            .title(texts::OVERLAY_SELECT_FIELD_TYPE)
            .borders(Borders::ALL)
            .style(ElementStyle::FieldTypeBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = Vec::new();

        for (i, item) in items.iter().enumerate() {
            let label = format!("{}. {}", i + 1, item.label());
            lines.push(Line::from(selected_span(&label, i == selected)));
        }

        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Draws the Custom field-name prompt.
    fn render_custom_field_name(&self, frame: &mut Frame, area: Rect) {
        let DetailPopup::CustomFieldName { input } = &self.popup else {
            return;
        };

        let overlay_area = centered_rect(area, area.width / 2, 4);

        let block = Block::default()
            .title(texts::OVERLAY_CUSTOM_FIELD_NAME)
            .borders(Borders::ALL)
            .style(ElementStyle::CustomNameBlock.style());

        let inner = block.inner(overlay_area);
        let lines = vec![
            Line::from(cursor_field_spans(
                texts::LABEL_NAME,
                input,
                input.len(),
                true,
            )),
            Line::from(""),
        ];

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Draws the delete-field confirmation.
    fn render_confirm_delete(&self, frame: &mut Frame, area: Rect, state: &TreeState) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let DetailPopup::ConfirmDeleteField { target_index } = self.popup else {
            return;
        };
        let label = form
            .node(&state.root)
            .extra_fields
            .get(target_index)
            .map(|ef| ef.field_type.label())
            .unwrap_or_default();

        let overlay_area = centered_rect(area, area.width / 2, 4);

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
        ];

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Badge and hint for the tree status bar while the modal is open.
    pub(crate) fn status_parts(&self, root: &Node) -> (&'static str, ElementStyle, &'static str) {
        match self.screen() {
            Screen::Browse => (
                texts::MODE_DETAIL,
                ElementStyle::ModeDetail,
                if self.alert.is_some() {
                    texts::HINT_DETAIL_ALERT
                } else {
                    texts::HINT_DETAIL
                },
            ),
            Screen::Form => {
                let form = self.form.as_ref().expect("the form is open");
                let (mode_str, mode_style) = match form.pending {
                    FormPending::Cut => (texts::MODE_PENDING_CUT, ElementStyle::ModePendingCut),
                    FormPending::Paste => (texts::MODE_PASTE, ElementStyle::ModePendingPaste),
                    FormPending::None if form.is_adding() => {
                        (texts::MODE_ADD, ElementStyle::ModeAdd)
                    }
                    FormPending::None => (texts::MODE_EDIT, ElementStyle::ModeEdit),
                };
                (mode_str, mode_style, form_hint(form, root))
            }
            Screen::SelectFieldType => (
                texts::MODE_FIELD_TYPE,
                ElementStyle::ModeFieldType,
                texts::HINT_SELECT_FIELD_TYPE,
            ),
            Screen::CustomFieldName => (
                texts::MODE_CUSTOM_NAME,
                ElementStyle::ModeCustomName,
                texts::HINT_CUSTOM_FIELD_NAME,
            ),
            Screen::GeneratePassword => (
                texts::MODE_GENERATOR,
                ElementStyle::ModeGenerator,
                texts::HINT_GENERATOR,
            ),
            Screen::SymbolChecklist => (
                texts::MODE_SYMBOLS,
                ElementStyle::ModeSymbols,
                texts::HINT_SYMBOLS,
            ),
            Screen::ConfirmDeleteField => (
                texts::MODE_CONFIRM,
                ElementStyle::ModeConfirmDelete,
                texts::HINT_CONFIRM_DELETE,
            ),
        }
    }

    // ── Test-only views into the modal's state ──

    /// The open Form, if any.
    #[cfg(test)]
    pub(crate) fn form(&self) -> Option<&FormState> {
        self.form.as_ref()
    }

    /// A mutable view of the open Form; panics if no Form is open.
    #[cfg(test)]
    pub(crate) fn form_mut(&mut self) -> &mut FormState {
        self.form.as_mut().expect("the form is open")
    }

    /// The generator modal, whether the parameter modal or the symbol checklist.
    #[cfg(test)]
    pub(crate) fn generator(&self) -> Option<&GeneratorModal> {
        match &self.popup {
            DetailPopup::GeneratePassword(modal) | DetailPopup::SymbolChecklist(modal) => {
                Some(modal)
            }
            _ => None,
        }
    }

    /// Whether the symbol checklist (not the parameter modal) is up.
    #[cfg(test)]
    pub(crate) fn is_symbol_checklist(&self) -> bool {
        matches!(self.popup, DetailPopup::SymbolChecklist(_))
    }

    /// Whether the field-type picker is up.
    #[cfg(test)]
    pub(crate) fn is_select_field_type(&self) -> bool {
        matches!(self.popup, DetailPopup::SelectFieldType { .. })
    }

    /// Focuses entry `index` of the field-type picker.
    #[cfg(test)]
    pub(crate) fn set_picker_selected(&mut self, index: usize) {
        if let DetailPopup::SelectFieldType { selected } = &mut self.popup {
            *selected = index;
        }
    }

    /// Opens the delete confirmation for the field at `target_index`.
    #[cfg(test)]
    pub(crate) fn set_confirm_delete(&mut self, target_index: usize) {
        self.popup = DetailPopup::ConfirmDeleteField { target_index };
    }
}

/// An entry offered by the field-type picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PickerItem {
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

/// Items offered by the field-type picker, in display order.
pub(crate) fn picker_items() -> Vec<PickerItem> {
    vec![
        PickerItem::Login,
        PickerItem::Password,
        PickerItem::Desc,
        PickerItem::Custom,
    ]
}

/// Which generator a field type has, if any.
///
/// An enum rather than a trait: a new variant forces every `match` to be
/// revisited. It is the single source of truth for both the key handler and the
/// form hint. It carries no parameters — those differ per field type (a login
/// word has none, a password has [`PasswordParams`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldGenerator {
    Login,
    Password,
}

impl FieldGenerator {
    /// The generator for `field_type`, or `None` when it has none.
    fn for_field(field_type: Option<&FieldType>) -> Option<Self> {
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
fn write_generated_value(state: &mut TreeState, form: &mut FormState, value: String) -> bool {
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
fn render_edit_overlay(frame: &mut Frame, area: Rect, form: &FormState, root: &Node, title: &str) {
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

/// Renders the password-generator parameter modal.
fn render_generate_password_overlay(frame: &mut Frame, area: Rect, modal: &GeneratorModal) {
    let has_error = modal.error.is_some();
    let content_height = 3 + u16::from(has_error);
    let overlay_height = (content_height + 2).min(area.height);
    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width / 2,
        height: overlay_height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_GENERATE_PASSWORD)
        .borders(Borders::ALL)
        .style(ElementStyle::GeneratorBlock.style());
    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    if modal.selected == GENERATOR_ROW_LENGTH {
        let mut spans = vec![Span::raw(Icon::SelectionArrow.as_str().to_string())];
        spans.extend(cursor_field_spans(
            texts::LABEL_GENERATOR_LENGTH,
            &modal.length_input,
            modal.length_cursor,
            true,
        ));
        lines.push(Line::from(spans));
    } else {
        let length_label = format!("{}{}", texts::LABEL_GENERATOR_LENGTH, modal.length_input);
        lines.push(Line::from(selected_span(&length_label, false)));
    }

    let selected = modal.params.symbols.len();
    let total = generator::SYMBOLS.len();
    let symbols_label = format!("{}: {selected}/{total}", texts::LABEL_GENERATOR_SYMBOLS);
    lines.push(Line::from(selected_span(
        &symbols_label,
        modal.selected == GENERATOR_ROW_SYMBOLS,
    )));

    lines.push(Line::from(selected_span(
        texts::LABEL_GENERATOR_RUN,
        modal.selected == GENERATOR_ROW_GENERATE,
    )));

    if modal.error.is_some() {
        lines.push(Line::from(Span::styled(
            texts::ERROR_GENERATOR_LENGTH,
            ElementStyle::GeneratorError.style(),
        )));
    }

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Renders the per-symbol checklist as a grid of checkbox cells.
fn render_symbol_checklist_overlay(frame: &mut Frame, area: Rect, modal: &GeneratorModal) {
    let rows = u16::try_from(generator::SYMBOLS.len().div_ceil(SYMBOL_COLUMNS))
        .expect("symbol checklist rows fit in a terminal height");
    let overlay_height = (rows + 2).min(area.height);
    let width = u16::try_from(SYMBOL_COLUMNS * 9)
        .expect("symbol checklist width fits in a terminal")
        .min(area.width);
    let overlay_area = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(overlay_height) / 2,
        width,
        height: overlay_height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SYMBOLS)
        .borders(Borders::ALL)
        .style(ElementStyle::SymbolsBlock.style());
    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    for row in 0..rows {
        let mut spans: Vec<Span> = Vec::new();
        for column in 0..SYMBOL_COLUMNS {
            let index = usize::from(row) * SYMBOL_COLUMNS + column;
            let Some(&symbol) = generator::SYMBOLS.get(index) else {
                break;
            };
            let marker = if modal.params.symbols.contains(symbol) {
                Icon::CheckboxOn
            } else {
                Icon::CheckboxOff
            };
            let style = if index == modal.selected {
                ElementStyle::ListRowSelected.style()
            } else {
                ElementStyle::CursorNormalText.style()
            };
            let cell = format!("{} {}", marker.as_str(), symbol);
            spans.push(Span::styled(format!("{cell:<9}"), style));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Draws the transient "copied" confirmation over `area`.
fn render_copy_alert(frame: &mut Frame, area: Rect) {
    let overlay_area = centered_rect(area, area.width / 3, 4);

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
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::{ExtraField, FieldType};
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
    fn y_copies_the_selected_value_and_alerts() {
        let (mut detail, mut state) = detail_with_fields();
        match detail.handle_key(key(KeyCode::Char('y')), &mut state) {
            Some(TreeAction::CopyToClipboard { text }) => assert_eq!(text, "a"),
            _ => panic!("expected a clipboard copy"),
        }
        assert_eq!(detail.alert.as_deref(), Some(texts::COPY_CHECK));
    }

    #[test]
    fn an_empty_value_skips_the_clipboard_but_still_alerts() {
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: String::new(),
        });
        let mut detail = Detail::browse();
        let mut state = TreeState::new(node);
        let _ = detail.handle_key(key(KeyCode::Char('y')), &mut state);
        assert_eq!(detail.alert.as_deref(), Some(texts::COPY_CHECK));
    }

    #[test]
    fn no_field_skips_both_the_clipboard_and_the_alert() {
        let mut detail = Detail::browse();
        let mut state = TreeState::new(Node::new("Root"));
        assert!(
            detail
                .handle_key(key(KeyCode::Char('y')), &mut state)
                .is_none()
        );
        assert!(detail.alert.is_none());
    }

    #[test]
    fn the_next_key_after_a_copy_alert_is_consumed() {
        let (mut detail, mut state) = detail_with_fields();
        let _ = detail.handle_key(key(KeyCode::Char('y')), &mut state);
        assert!(detail.handle_key(key(KeyCode::Esc), &mut state).is_none());
        assert!(detail.alert.is_none());
    }

    #[test]
    fn status_parts_switch_hint_while_the_alert_is_armed() {
        let (mut detail, mut state) = detail_with_fields();
        assert_eq!(detail.status_parts(&state.root).2, texts::HINT_DETAIL);
        let _ = detail.handle_key(key(KeyCode::Char('y')), &mut state);
        assert_eq!(detail.status_parts(&state.root).2, texts::HINT_DETAIL_ALERT);
    }
}
