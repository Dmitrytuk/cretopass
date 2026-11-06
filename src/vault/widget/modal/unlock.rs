//! The unlock prompt: one master-password field for an encrypted file.
//!
//! The prompt holds the file it was opened for, so the orchestrator decrypts
//! exactly that file once the user asks to unlock.

use std::borrow::Cow;

use zeroize::Zeroizing;

use super::HomeAction;
use crate::cursor::{FieldEdit, SecretFieldGroup, secret_field_spans};
use crate::hitbox::ClickSurface;
use crate::theming::{Action, ElementStyle, texts};
use crate::vault::widget::chrome::{display_path, make_overlay, paint_overlay};
use crossterm::event::{KeyEvent, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

/// The prompt for the master password of one encrypted file.
pub struct UnlockPrompt {
    file_path: String,
    file_data: Vec<u8>,
    fields: SecretFieldGroup,
    error: Option<String>,
    /// The clickable field rows and the panel the renderer drew, so a left click
    /// focuses a field and a click outside the panel dismisses the prompt.
    clicks: ClickSurface<usize>,
}

impl UnlockPrompt {
    /// A prompt for `file_path`, whose encrypted bytes are `file_data`.
    pub fn new(file_path: String, file_data: Vec<u8>) -> Self {
        Self {
            file_path,
            file_data,
            fields: SecretFieldGroup::with_fields(1),
            error: None,
            clicks: ClickSurface::default(),
        }
    }

    /// The file this prompt unlocks.
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// The encrypted bytes of [`UnlockPrompt::file_path`].
    pub fn file_data(&self) -> &[u8] {
        &self.file_data
    }

    /// The typed master password, in a buffer that wipes itself when dropped.
    pub fn typed(&self) -> Zeroizing<String> {
        Zeroizing::new(self.fields.field(0).text().to_string())
    }

    /// Shows `message` and puts the cursor back at the start of the field,
    /// keeping what the user typed so it can be corrected.
    pub fn reject(&mut self, message: String) {
        self.error = Some(message);
        self.fields.reset_cursors();
    }

    /// `Ctrl+S` reveals the password, `Enter` asks to unlock, `Esc` dismisses.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match Action::from_passphrase_input(&key) {
            Action::Cancel => Some(HomeAction::Close),
            Action::Confirm => Some(HomeAction::Unlock),
            action => {
                // Typing again clears the previous "wrong password" report.
                if self.fields.apply_action(action) == FieldEdit::Inserted {
                    self.error = None;
                }
                None
            }
        }
    }

    /// Pastes the clipboard's text into the password field, single-line only.
    pub fn handle_paste(&mut self, text: &str) {
        if self.fields.paste_clipboard(text) {
            self.error = None;
        }
    }

    /// A left click on the field focuses it; any other button is ignored.
    pub fn handle_click(&mut self, button: MouseButton, x: u16, y: u16) {
        if let Some(index) = self.clicks.target_at(button, x, y) {
            self.fields.focus(index);
        }
    }

    /// The panel the renderer drew last frame, or `None` before the first frame.
    /// A left click outside it dismisses the prompt, like `Esc`.
    pub(crate) fn panel_rect(&self) -> Option<Rect> {
        self.clicks.panel()
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, display_full_path: bool) {
        self.clicks.clear();
        let shown_path = display_path(&self.file_path, display_full_path);

        let overlay_area = make_overlay(area, if self.error.is_some() { 8 } else { 7 });
        self.clicks.set_panel(overlay_area);
        let block = Block::default()
            .title(texts::HOME_ENTER_PASSPHRASE)
            .borders(Borders::ALL)
            .style(ElementStyle::HomePassBlock.style());

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} {}", texts::LABEL_FILE, shown_path),
            ElementStyle::CursorLabel.style(),
        )));
        lines.push(Line::from(""));

        let mut field_spans = vec![Span::styled(
            texts::LABEL_PASSPHRASE,
            ElementStyle::HomePassLabel.style(),
        )];
        field_spans.extend(secret_field_spans(
            self.fields.field(0),
            true,
            self.fields.show(),
        ));
        lines.push(Line::from(field_spans));

        if let Some(message) = &self.error {
            lines.push(Line::from(Span::styled(
                format!(" {}", message),
                ElementStyle::HomeErrorBar.style(),
            )));
        }

        lines.push(Line::from(""));

        let inner = block.inner(overlay_area);
        if inner.height > 3 {
            self.clicks
                .record(Rect::new(inner.x, inner.y + 3, inner.width, 1), 0);
        }

        paint_overlay(
            frame,
            overlay_area,
            block,
            Paragraph::new(Text::from(lines)),
        );
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        (
            texts::MODE_PASSPHRASE,
            ElementStyle::ModeEdit,
            Cow::Borrowed(texts::HINT_PASSPHRASE),
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

    fn prompt() -> UnlockPrompt {
        UnlockPrompt::new("/tmp/vault.age".to_string(), vec![1, 2, 3])
    }

    #[test]
    fn typing_then_enter_asks_to_unlock() {
        let mut prompt = prompt();
        for c in "secret".chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::Unlock)
        ));
        assert_eq!(prompt.typed().as_str(), "secret");
    }

    #[test]
    fn a_rejection_keeps_the_text_but_resets_the_cursor() {
        let mut prompt = prompt();
        for c in "secret".chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }
        prompt.reject("wrong".to_string());

        assert_eq!(prompt.typed().as_str(), "secret");
        assert_eq!(prompt.fields.field(0).cursor(), 0);
    }

    #[test]
    fn typing_again_clears_the_error() {
        let mut prompt = prompt();
        prompt.reject("wrong".to_string());
        prompt.handle_key(key(KeyCode::Char('a')));
        assert!(prompt.error.is_none());
    }

    #[test]
    fn esc_dismisses_the_prompt() {
        let mut prompt = prompt();
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn the_prompt_keeps_the_file_it_was_opened_for() {
        let prompt = prompt();
        assert_eq!(prompt.file_path(), "/tmp/vault.age");
        assert_eq!(prompt.file_data(), &[1, 2, 3]);
    }
}
