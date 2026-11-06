//! The Detail modal's picker screens: the field-type picker, the custom
//! field-name prompt, and the delete-field confirmation.
//!
//! All three are nested over the open Form. The picker chooses which field type
//! to append, the custom-name prompt collects a label for a `Custom` field, and
//! the confirmation removes a field.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::super::chrome::centered_rect;
use super::form::commit_form_change;
use super::{Detail, DetailHit, DetailPopup, TreeAction, plain_key};
use crate::cursor::{cursor_field_spans, push_char_capped, selected_span};
use crate::hitbox::{RowClick, record_rows};
use crate::theming::{Action, ElementStyle, texts};
use crate::tree::node::{ExtraField, FieldType};
use crate::tree::widget::{FormFocus, TreeState};

impl Detail {
    /// Keys for the field-type picker opened from the `+ Add Field` row.
    pub(super) fn handle_select_field_type(
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
    pub(super) fn handle_custom_field_name(
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
                push_char_capped(&mut input, c);
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

    /// Keys for the delete-field confirmation popup.
    pub(super) fn handle_confirm_delete(
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

    /// A click on a field-type picker entry: select it, or on a repeat choose it.
    pub(super) fn click_picker_item(
        &mut self,
        index: usize,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        if index >= picker_items().len() {
            return None;
        }
        let click = match &self.popup {
            DetailPopup::SelectFieldType { selected } => RowClick::resolve(*selected, index),
            // A different popup is not this picker: the old `matches!` guard was
            // false here, so the click only moves and never activates.
            _ => RowClick::Moved,
        };
        if let DetailPopup::SelectFieldType { selected } = &mut self.popup {
            *selected = index;
        }
        if click.activated() {
            self.handle_key(plain_key(KeyCode::Enter), state)
        } else {
            None
        }
    }

    /// Draws the field-type picker.
    pub(super) fn render_select_field_type(&mut self, frame: &mut Frame, area: Rect) {
        if self.form.is_none() {
            return;
        }
        let DetailPopup::SelectFieldType { selected } = self.popup else {
            return;
        };
        let items = picker_items();

        let overlay_area = centered_rect(area, area.width / 2, 8);
        self.clicks.set_panel(overlay_area);

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

        let visible = items.len().min(inner.height as usize);
        record_rows(&mut self.clicks, inner, 0, visible, DetailHit::PickerItem);
    }

    /// Draws the Custom field-name prompt.
    pub(super) fn render_custom_field_name(&mut self, frame: &mut Frame, area: Rect) {
        let DetailPopup::CustomFieldName { input } = &self.popup else {
            return;
        };

        let overlay_area = centered_rect(area, area.width / 2, 4);
        self.clicks.set_panel(overlay_area);

        let block = Block::default()
            .title(texts::OVERLAY_CUSTOM_FIELD_NAME)
            .borders(Borders::ALL)
            .style(ElementStyle::CustomNameBlock.style());

        let inner = block.inner(overlay_area);
        let lines = vec![
            Line::from(cursor_field_spans(texts::LABEL_NAME, input, input.len())),
            Line::from(""),
        ];

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Draws the delete-field confirmation.
    pub(super) fn render_confirm_delete(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &TreeState,
    ) {
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
        self.clicks.set_panel(overlay_area);

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

    /// Whether the field-type picker is up.
    #[cfg(test)]
    pub(crate) fn is_select_field_type(&self) -> bool {
        matches!(self.popup, DetailPopup::SelectFieldType { .. })
    }

    /// Whether the delete-field confirmation is up.
    #[cfg(test)]
    pub(crate) fn is_confirm_delete(&self) -> bool {
        matches!(self.popup, DetailPopup::ConfirmDeleteField { .. })
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
