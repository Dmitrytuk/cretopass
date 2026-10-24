//! The change-master-password prompt: current, new, and confirmation.
//!
//! Validation belongs to the orchestrator here: the "current" field is checked
//! against the open vault's master password, which only the orchestrator holds.
//! The prompt only reports what was typed.

use std::borrow::Cow;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::HomeAction;
use crate::theming::{Action, ElementStyle, texts};
use crate::vault::widget::chrome::{make_overlay, paint_overlay};
use crate::vault::widget::secret_field::{SecretField, SecretFieldGroup, password_field_spans};

/// The labels of the three fields, in focus order.
const LABELS: [&str; 3] = [
    texts::LABEL_CURRENT_PW,
    texts::LABEL_NEW_PW,
    texts::LABEL_CONFIRM_PW,
];

/// The dialog that types a replacement master password.
pub struct ChangePasswordPrompt {
    fields: SecretFieldGroup,
    error: Option<String>,
}

impl ChangePasswordPrompt {
    /// A dialog with the three fields empty, the current one focused.
    pub fn new() -> Self {
        Self {
            fields: SecretFieldGroup::with_fields(3),
            error: None,
        }
    }

    /// What the user typed as the current master password.
    pub fn current(&self) -> &str {
        self.fields.field(0).text()
    }

    /// What the user typed as the new master password.
    pub fn new_password(&self) -> &str {
        self.fields.field(1).text()
    }

    /// What the user typed as the confirmation of the new master password.
    pub fn confirmation(&self) -> &str {
        self.fields.field(2).text()
    }

    /// Shows `message`, leaving what was typed in place to be corrected.
    pub fn reject(&mut self, message: String) {
        self.error = Some(message);
    }

    /// Shows `message` and empties all three fields — the "start again" path.
    pub fn reject_and_clear(&mut self, message: String) {
        self.error = Some(message);
        self.fields.clear();
    }

    /// `Tab` moves between the fields, `Ctrl+S` reveals all three, `Esc` dismisses.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match Action::from_change_pw_input(&key) {
            Action::Cancel => Some(HomeAction::Close),
            Action::NextField => {
                self.fields.next_field();
                None
            }
            Action::PrevField => {
                self.fields.prev_field();
                None
            }
            Action::ToggleShowPassword => {
                self.fields.toggle_show();
                None
            }
            Action::InsertChar(c) => self.edit(|field| field.insert(c)),
            Action::Backspace => self.edit(SecretField::backspace),
            Action::DeleteChar => self.edit(SecretField::delete),
            Action::CursorLeft => self.move_cursor(SecretField::move_left),
            Action::CursorRight => self.move_cursor(SecretField::move_right),
            Action::CursorHome => self.move_cursor(SecretField::move_home),
            Action::CursorEnd => self.move_cursor(SecretField::move_end),
            // The orchestrator checks the three fields against the open vault.
            Action::Confirm => Some(HomeAction::ChangeMasterPassword),
            _ => None,
        }
    }

    /// Edits the focused field and clears the previous report.
    fn edit(&mut self, action: impl FnOnce(&mut SecretField)) -> Option<HomeAction> {
        self.fields.apply(action);
        self.error = None;
        None
    }

    /// Moves the cursor in the focused field, leaving the report untouched.
    fn move_cursor(&mut self, action: impl FnOnce(&mut SecretField)) -> Option<HomeAction> {
        self.fields.apply(action);
        None
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = make_overlay(area, if self.error.is_some() { 12 } else { 11 });
        let block = Block::default()
            .title(texts::HOME_CHANGE_PASSWORD)
            .borders(Borders::ALL)
            .style(ElementStyle::HomeChangePwBlock.style());

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        for (index, label) in LABELS.iter().enumerate() {
            lines.push(Line::from(password_field_spans(
                label,
                self.fields.field(index),
                index == self.fields.focused_index(),
                self.fields.show(),
            )));
        }

        if let Some(message) = &self.error {
            lines.push(Line::from(Span::styled(
                format!(" {}", message),
                ElementStyle::HomeErrorBar.style(),
            )));
        }

        lines.push(Line::from(""));

        paint_overlay(
            frame,
            overlay_area,
            block,
            Paragraph::new(Text::from(lines)),
        );
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        (
            texts::MODE_CHANGE_PW,
            ElementStyle::ModeConfirmDelete,
            Cow::Borrowed(texts::HINT_CHANGE_PW_INPUT),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    #[test]
    fn enter_reports_the_three_fields_and_leaves_validation_to_the_caller() {
        let mut prompt = ChangePasswordPrompt::new();
        for c in "old".chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }
        prompt.handle_key(key(KeyCode::Tab));
        for c in "new".chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }

        assert!(matches!(
            prompt.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::ChangeMasterPassword)
        ));
        assert_eq!(prompt.current(), "old");
        assert_eq!(prompt.new_password(), "new");
        assert_eq!(prompt.confirmation(), "");
    }

    #[test]
    fn tab_cycles_through_the_three_fields() {
        let mut prompt = ChangePasswordPrompt::new();
        prompt.handle_key(key(KeyCode::Tab));
        assert_eq!(prompt.fields.focused_index(), 1);
        prompt.handle_key(key(KeyCode::Tab));
        assert_eq!(prompt.fields.focused_index(), 2);
        prompt.handle_key(key(KeyCode::Tab));
        assert_eq!(prompt.fields.focused_index(), 0);
    }

    #[test]
    fn a_rejection_keeps_the_text_and_a_rejection_with_clear_does_not() {
        let mut prompt = ChangePasswordPrompt::new();
        prompt.handle_key(key(KeyCode::Char('o')));
        prompt.reject("wrong".to_string());
        assert_eq!(prompt.current(), "o");

        prompt.reject_and_clear("failed".to_string());
        assert_eq!(prompt.current(), "");
        assert_eq!(prompt.new_password(), "");
        assert_eq!(prompt.confirmation(), "");
        assert_eq!(prompt.error.as_deref(), Some("failed"));
    }

    #[test]
    fn esc_dismisses_the_dialog() {
        let mut prompt = ChangePasswordPrompt::new();
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }
}
