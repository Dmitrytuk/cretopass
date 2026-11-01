//! The create-secret-file prompt: the new master password and its confirmation.
//!
//! It validates the pair itself — that needs nothing outside the prompt — and
//! asks the orchestrator to create the file only once the pair is usable.

use std::borrow::Cow;

use super::HomeAction;
use crate::cursor::{FieldEdit, PairVerdict, SecretFieldGroup, password_field_spans};
use crate::theming::{Action, ElementStyle, texts};
use crate::vault::widget::chrome::{display_path, make_overlay, paint_overlay};
use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

/// The dialog that types the master password for a file that is about to exist.
pub struct CreateSecretPrompt {
    file_path: String,
    fields: SecretFieldGroup,
    error: Option<String>,
}

impl CreateSecretPrompt {
    /// A dialog for the file about to be created at `file_path`.
    pub fn new(file_path: String) -> Self {
        Self {
            file_path,
            fields: SecretFieldGroup::with_fields(2),
            error: None,
        }
    }

    /// The file about to be created.
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// The typed master password.
    pub fn typed(&self) -> String {
        self.fields.field(0).text().to_string()
    }

    /// Shows `message`, puts every cursor back at the start and keeps the text and
    /// the reveal flag, so the pair can be corrected.
    pub fn reject(&mut self, message: String) {
        self.error = Some(message);
        self.fields.reset_cursors();
    }

    /// `Tab` moves between the two fields, `Ctrl+S` reveals both, `Esc` dismisses.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match Action::from_create_secret_file(&key) {
            Action::Cancel => Some(HomeAction::Close),
            Action::Confirm => self.confirm(),
            action => {
                if matches!(
                    self.fields.apply_action(action),
                    FieldEdit::Inserted | FieldEdit::Deleted
                ) {
                    self.error = None;
                }
                None
            }
        }
    }

    /// Validates the pair: empty means "no passphrase" (a separate confirmation),
    /// mismatched re-shows the dialog, anything else can be created.
    fn confirm(&mut self) -> Option<HomeAction> {
        match self.fields.pair_verdict() {
            PairVerdict::Empty => Some(HomeAction::ConfirmNoPassphrase {
                file_path: self.file_path.clone(),
            }),
            PairVerdict::Mismatch => {
                self.reject(texts::MSG_PW_MISMATCH.to_string());
                None
            }
            PairVerdict::Match => Some(HomeAction::CreateSecret),
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, display_full_path: bool) {
        let shown_path = display_path(&self.file_path, display_full_path);

        let overlay_area = make_overlay(area, if self.error.is_some() { 10 } else { 9 });
        let block = Block::default()
            .title(texts::HOME_CREATE_SECRET)
            .borders(Borders::ALL)
            .style(ElementStyle::HomePassBlock.style());

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} {}", texts::LABEL_FILE, shown_path),
            ElementStyle::CursorLabel.style(),
        )));
        lines.push(Line::from(""));

        for (index, label) in [
            texts::LABEL_PASSPHRASE_FIELD,
            texts::LABEL_PASSPHRASE_CONFIRM,
        ]
        .iter()
        .enumerate()
        {
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
            texts::MODE_CREATE,
            ElementStyle::ModeEdit,
            Cow::Borrowed(texts::HINT_CREATE_SECRET),
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

    fn typed(prompt: &mut CreateSecretPrompt, passphrase: &str, confirmation: &str) {
        for c in passphrase.chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }
        prompt.handle_key(key(KeyCode::Tab));
        for c in confirmation.chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }
        prompt.handle_key(key(KeyCode::Tab));
    }

    #[test]
    fn a_matching_pair_asks_to_create_the_file() {
        let mut prompt = CreateSecretPrompt::new("/tmp/new.age".to_string());
        typed(&mut prompt, "pw", "pw");
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::CreateSecret)
        ));
        assert_eq!(prompt.typed(), "pw");
    }

    #[test]
    fn a_mismatched_pair_re_shows_the_dialog() {
        let mut prompt = CreateSecretPrompt::new("/tmp/new.age".to_string());
        typed(&mut prompt, "pw", "other");

        assert!(prompt.handle_key(key(KeyCode::Enter)).is_none());
        assert_eq!(prompt.error.as_deref(), Some(texts::MSG_PW_MISMATCH));
        assert_eq!(prompt.fields.field(0).text(), "pw");
        assert_eq!(prompt.fields.field(0).cursor(), 0, "cursors are reset");
    }

    #[test]
    fn an_empty_pair_asks_about_a_passwordless_file() {
        let mut prompt = CreateSecretPrompt::new("/tmp/new.age".to_string());
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::ConfirmNoPassphrase { file_path }) if file_path == "/tmp/new.age"
        ));
    }

    #[test]
    fn tab_moves_between_the_fields_and_back() {
        let mut prompt = CreateSecretPrompt::new("/tmp/new.age".to_string());
        prompt.handle_key(key(KeyCode::Tab));
        assert_eq!(prompt.fields.focused_index(), 1);
        prompt.handle_key(key(KeyCode::BackTab));
        assert_eq!(prompt.fields.focused_index(), 0);
    }

    #[test]
    fn editing_clears_the_previous_report() {
        let mut prompt = CreateSecretPrompt::new("/tmp/new.age".to_string());
        typed(&mut prompt, "pw", "other");
        prompt.handle_key(key(KeyCode::Enter));
        assert!(prompt.error.is_some());

        prompt.handle_key(key(KeyCode::Backspace));
        assert!(prompt.error.is_none());
    }

    #[test]
    fn esc_dismisses_the_dialog() {
        let mut prompt = CreateSecretPrompt::new("/tmp/new.age".to_string());
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }
}
