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
//!
//! This facade owns the modal's state, its screen dispatch, and the shared
//! constructors. Each screen keeps its key handling and drawing in its own
//! submodule: [`browse`] (the field list), [`form`] (the Add/Edit form),
//! [`picker`] (the field-type picker, the custom-name prompt and the delete
//! confirmation) and [`generator`] (the password generator and its symbol
//! checklist).

mod browse;
mod form;
mod generator;
mod picker;
#[cfg(test)]
mod tests;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;

use super::TreeAction;
use crate::cursor::{PastePolicy, insert_str_at, push_str_capped};
use crate::hitbox::ClickSurface;
use crate::theming::{ElementStyle, texts};
use crate::tree::node::Node;
use crate::tree::widget::{FormPending, FormState, GeneratorModal, TreeState};

// `form_hint` is used by `status_parts`; the rest are reached only by the
// tree's tests (via `modal`'s test-only re-export), so they are gated the same
// way to keep the production build free of unused re-exports.
pub(crate) use self::form::form_hint;
#[cfg(test)]
pub(crate) use self::form::render_extra_field_line;
#[cfg(test)]
pub(crate) use self::picker::{PickerItem, picker_items};

/// A key with no modifiers, for a click that stands in for a key press.
fn plain_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// Rows of the password-generator modal, in display order.
pub(crate) const GENERATOR_ROW_LENGTH: usize = 0;
pub(crate) const GENERATOR_ROW_SYMBOLS: usize = 1;
pub(crate) const GENERATOR_ROW_GENERATE: usize = 2;

/// The Detail modal: browse a selected Node's ExtraFields, copy a value, or edit.
#[derive(Debug, Default)]
pub(crate) struct Detail {
    /// Highlighted extra-field row while browsing.
    selected: usize,
    /// The open Form Popup, or `None` while browsing. Held inside the generator
    /// popups rather than here while one of those is up.
    form: Option<FormState>,
    /// The Popup nested over the Form, if any.
    popup: DetailPopup,
    /// The clickable regions this modal's renderer recorded for the last frame,
    /// together with the topmost panel it drew. An outside click closes exactly
    /// one nesting level, matching `Esc`.
    clicks: ClickSurface<DetailHit>,
}

/// A clickable region of the Detail modal, recorded by its renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DetailHit {
    /// A browsed extra-field row.
    Field(usize),
    /// A row of the Add/Edit form.
    FormRow(usize),
    /// An entry of the field-type picker.
    PickerItem(usize),
    /// A parameter row of the password-generator modal.
    GeneratorRow(usize),
    /// A cell of the symbol checklist.
    SymbolCell(usize),
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
            form: Some(FormState::new_edit(node)),
            popup: DetailPopup::None,
            clicks: ClickSurface::default(),
        }
    }

    /// A modal creating a new Node in its Form Popup.
    pub(crate) fn adding() -> Self {
        Self {
            selected: 0,
            form: Some(FormState::new_add()),
            popup: DetailPopup::None,
            clicks: ClickSurface::default(),
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

    /// Applies pasted `text` to the modal's active typing surface.
    ///
    /// Only the three surfaces that accept typed text take a paste: the Form's
    /// scratch buffer at its Input level, the Custom field-name prompt, and the
    /// generator's length row (digits only). Browsing and the passive screens
    /// ignore it. `state` is part of the modal contract and unused here: every
    /// surface pastes into buffers the modal already owns.
    pub(crate) fn handle_paste(&mut self, text: &str, _state: &mut TreeState) {
        match self.screen() {
            Screen::Form => {
                PastePolicy::SingleLine.apply_cleaned(text, |cleaned| {
                    if cleaned.is_empty() {
                        return;
                    }
                    let Some(scratch) = self.form.as_mut().and_then(|form| form.scratch.as_mut())
                    else {
                        return;
                    };
                    insert_str_at(&mut scratch.text, &mut scratch.cursor, cleaned);
                });
            }
            Screen::CustomFieldName => {
                let cleaned = PastePolicy::SingleLine.clean(text);
                if cleaned.is_empty() {
                    return;
                }
                if let DetailPopup::CustomFieldName { input } = &mut self.popup {
                    push_str_capped(input, &cleaned);
                }
            }
            Screen::GeneratePassword => {
                let cleaned = PastePolicy::Digits.clean(text);
                if cleaned.is_empty() {
                    return;
                }
                if let DetailPopup::GeneratePassword(modal) = &mut self.popup
                    && modal.selected == GENERATOR_ROW_LENGTH
                {
                    generator::paste_generator_length(modal, &cleaned);
                }
            }
            Screen::Browse
            | Screen::SelectFieldType
            | Screen::SymbolChecklist
            | Screen::ConfirmDeleteField => {}
        }
    }

    /// Applies a click at `(x, y)` by hit-testing the regions the renderer
    /// recorded and then activating the target the way its key would.
    ///
    /// The click first moves the selection to the clicked row; a repeat click on
    /// the already-selected row activates it. The right button is meaningful only
    /// on a browsed field row, where it copies the field's value.
    pub(crate) fn handle_click(
        &mut self,
        button: MouseButton,
        x: u16,
        y: u16,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let target = self.clicks.target_at(button, x, y)?;
        match target {
            DetailHit::Field(index) => self.click_field(index, button, state),
            DetailHit::FormRow(index) if crate::hitbox::is_left(button) => {
                self.click_form_row(index, state)
            }
            DetailHit::PickerItem(index) if crate::hitbox::is_left(button) => {
                self.click_picker_item(index, state)
            }
            DetailHit::GeneratorRow(index) if crate::hitbox::is_left(button) => {
                self.click_generator_row(index, state)
            }
            DetailHit::SymbolCell(index) if crate::hitbox::is_left(button) => {
                self.click_symbol_cell(index, state)
            }
            _ => None,
        }
    }

    /// The topmost overlay drawn last frame: the Form, a nested Popup, or the
    /// card being browsed.
    ///
    /// The containment check in [`Modal::handle_click`](super::Modal::handle_click)
    /// uses this to turn a click outside the active overlay into one `Esc`, which
    /// is what unwinds exactly one nesting level.
    pub(crate) fn panel_rect(&self) -> Option<Rect> {
        self.clicks.panel()
    }

    /// Draws the active screen over `area`, refilling the clickable regions.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect, state: &TreeState) {
        // Reset before drawing: a screen that bails out (no Node, no Form) must
        // leave the panel as `None`, so it cannot be dismissed by a stray click.
        self.clicks.clear();
        match self.screen() {
            Screen::Browse => self.render_browse(frame, area, state),
            Screen::Form => self.render_form(frame, area, state),
            Screen::SelectFieldType => self.render_select_field_type(frame, area),
            Screen::CustomFieldName => self.render_custom_field_name(frame, area),
            Screen::GeneratePassword => {
                if let DetailPopup::GeneratePassword(modal) = &self.popup {
                    let panel = generator::render_generate_password_overlay(
                        frame,
                        area,
                        modal,
                        &mut self.clicks,
                    );
                    self.clicks.set_panel(panel);
                }
            }
            Screen::SymbolChecklist => {
                if let DetailPopup::SymbolChecklist(modal) = &self.popup {
                    let panel = generator::render_symbol_checklist_overlay(
                        frame,
                        area,
                        modal,
                        &mut self.clicks,
                    );
                    self.clicks.set_panel(panel);
                }
            }
            Screen::ConfirmDeleteField => self.render_confirm_delete(frame, area, state),
        }
    }

    /// Badge and hint for the tree status bar while the modal is open.
    pub(crate) fn status_parts(&self, root: &Node) -> (&'static str, ElementStyle, &'static str) {
        match self.screen() {
            Screen::Browse => (
                texts::MODE_DETAIL,
                ElementStyle::ModeDetail,
                texts::HINT_DETAIL,
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
}
