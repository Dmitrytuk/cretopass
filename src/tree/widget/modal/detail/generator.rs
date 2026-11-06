//! The Detail modal's password-generator screens: the parameter modal and the
//! per-symbol checklist nested over it.
//!
//! `Ctrl+G` on a Password field opens the parameter modal (length + a symbol
//! summary row that expands into the checklist). Confirming writes the generated
//! value into the focused field through the form's commit helpers.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::form::{FieldGenerator, write_generated_value};
use super::{
    Detail, DetailHit, DetailPopup, GENERATOR_ROW_GENERATE, GENERATOR_ROW_LENGTH,
    GENERATOR_ROW_SYMBOLS, TreeAction, plain_key,
};
use crate::cursor::{
    backspace_at, cursor_field_spans, cursor_left, cursor_right, delete_at, insert_char_at,
    insert_str_at, selected_span,
};
use crate::generator::{self, PasswordLength, PasswordParams};
use crate::hitbox::{ClickSurface, RowClick, record_rows};
use crate::theming::{Action, ElementStyle, Icon, texts};
use crate::tree::widget::{GeneratorModal, TreeState};

/// Rows of the password-generator parameter modal, in display order.
const GENERATOR_ROWS: usize = 3;

/// Columns in the symbol checklist grid.
const SYMBOL_COLUMNS: usize = 3;

impl Detail {
    /// Keys for the password-generator parameter modal.
    ///
    /// `Ctrl+G` opened this from the Add/Edit form's Input level. On the length
    /// row, the first digit replaces the seeded value and the caret can then be
    /// moved with `Left`/`Right`/`Home`/`End`, inserting/removing at most three
    /// digits. Generating writes the value into the focused field (and its
    /// scratch buffer), commits it (undo entry + autosave) and returns to the
    /// form still editing the field. `Esc` returns without touching the field.
    pub(super) fn handle_generate_password(
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
    pub(super) fn handle_symbol_checklist(
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

    /// A click on a generator parameter row: select it, or on a repeat act on it.
    pub(super) fn click_generator_row(
        &mut self,
        index: usize,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        if index >= GENERATOR_ROWS {
            return None;
        }
        let click = match &self.popup {
            DetailPopup::GeneratePassword(modal) => RowClick::resolve(modal.selected, index),
            // A different popup is not this generator: the old `matches!` guard
            // was false here, so the click only moves and never activates.
            _ => RowClick::Moved,
        };
        if let DetailPopup::GeneratePassword(modal) = &mut self.popup {
            modal.selected = index;
        }
        if click.activated() {
            self.handle_key(plain_key(KeyCode::Enter), state)
        } else {
            None
        }
    }

    /// A click on a symbol-checklist cell selects and toggles it in one gesture.
    pub(super) fn click_symbol_cell(
        &mut self,
        index: usize,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        if index >= generator::SYMBOLS.len() {
            return None;
        }
        if let DetailPopup::SymbolChecklist(modal) = &mut self.popup {
            modal.selected = index;
        }
        self.handle_key(plain_key(KeyCode::Char(' ')), state)
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
}

/// Pastes `digits` into the generator's length field.
///
/// Mirrors typing a digit on the length row: a fresh field is replaced rather
/// than appended to, and the three-digit cap is preserved. `digits` is already
/// reduced to ASCII digits by [`PastePolicy::Digits`](crate::cursor::PastePolicy).
pub(super) fn paste_generator_length(modal: &mut GeneratorModal, digits: &str) {
    if modal.length_fresh {
        modal.length_input.clear();
        modal.length_cursor = 0;
        modal.length_fresh = false;
    }
    // The field holds at most three ASCII digits, so one bounded slice can be
    // inserted at once instead of filtering digit by digit.
    let room = 3usize.saturating_sub(modal.length_input.len());
    let paste: String = digits.chars().take(room).collect();
    insert_str_at(&mut modal.length_input, &mut modal.length_cursor, &paste);
    modal.error = None;
}

/// Renders the password-generator parameter modal.
pub(super) fn render_generate_password_overlay(
    frame: &mut Frame,
    area: Rect,
    modal: &GeneratorModal,
    clicks: &mut ClickSurface<DetailHit>,
) -> Rect {
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

    if let Some(error) = modal.error {
        lines.push(Line::from(Span::styled(
            error.to_string(),
            ElementStyle::GeneratorError.style(),
        )));
    }

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);

    // The three parameter rows are the overlay's first three inner lines; a
    // terminal shorter than that shows fewer of them.
    let visible = GENERATOR_ROWS.min(inner.height as usize);
    record_rows(clicks, inner, 0, visible, DetailHit::GeneratorRow);

    overlay_area
}

/// Renders the per-symbol checklist as a grid of checkbox cells.
pub(super) fn render_symbol_checklist_overlay(
    frame: &mut Frame,
    area: Rect,
    modal: &GeneratorModal,
    clicks: &mut ClickSurface<DetailHit>,
) -> Rect {
    let rows = u16::try_from(generator::SYMBOLS.len().div_ceil(SYMBOL_COLUMNS))
        .expect("symbol checklist rows fit in a terminal height");
    let overlay_height = (rows + 2).min(area.height);
    // `+ 2` for the borders: the inner width must hold all three 9-wide columns,
    // or the last column would be drawn but not clickable.
    let width = u16::try_from(SYMBOL_COLUMNS * 9 + 2)
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

    // One clickable cell per symbol, nine columns wide, in the grid's own rows.
    // Bound by the drawn card so a tiny terminal cannot record cells off it.
    for row in 0..rows.min(inner.height) {
        for column in 0..SYMBOL_COLUMNS {
            if (column as u16 + 1) * 9 > inner.width {
                break;
            }
            let index = usize::from(row) * SYMBOL_COLUMNS + column;
            if index >= generator::SYMBOLS.len() {
                break;
            }
            let rect = Rect::new(inner.x + column as u16 * 9, inner.y + row, 9, 1);
            clicks.record(rect, DetailHit::SymbolCell(index));
        }
    }

    overlay_area
}
