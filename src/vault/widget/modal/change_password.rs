//! The change-master-password prompt: current, new, and confirmation.
//!
//! Validation belongs to the orchestrator here: the "current" field is checked
//! against the open vault's master password, which only the orchestrator holds.
//! The prompt only reports what was typed.

use std::borrow::Cow;

use crossterm::event::{KeyEvent, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::HomeAction;
use crate::cursor::{FieldEdit, SecretFieldGroup, password_field_spans};
use crate::hitbox::{HitMap, is_left};
use crate::theming::{Action, ElementStyle, texts};
use crate::vault::widget::chrome::{make_overlay, paint_overlay};

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
    /// The clickable field rows recorded by the renderer.
    hits: HitMap<usize>,
    /// The panel the renderer drew last frame, or `None` before the first frame.
    /// A left click outside it dismisses the dialog, like `Esc`.
    rect: Option<Rect>,
}

impl ChangePasswordPrompt {
    /// A dialog with the three fields empty, the current one focused.
    pub fn new() -> Self {
        Self {
            fields: SecretFieldGroup::with_fields(LABELS.len()),
            error: None,
            hits: HitMap::default(),
            rect: None,
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
            // The orchestrator checks the three fields against the open vault.
            Action::Confirm => Some(HomeAction::ChangeMasterPassword),
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

    /// Pastes the clipboard's text into the focused password field,
    /// single-line only.
    pub fn handle_paste(&mut self, text: &str) {
        if self.fields.paste_clipboard(text) {
            self.error = None;
        }
    }

    /// A left click on a field row focuses that field; the right button is
    /// ignored.
    pub fn handle_click(&mut self, button: MouseButton, x: u16, y: u16) {
        if !is_left(button) {
            return;
        }
        if let Some(index) = self.hits.hit(x, y, button) {
            self.fields.focus(index);
        }
    }

    /// The panel the renderer drew last frame, or `None` before the first frame.
    /// A left click outside it dismisses the dialog, like `Esc`.
    pub(crate) fn panel_rect(&self) -> Option<Rect> {
        self.rect
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.hits.clear();
        let overlay_area = make_overlay(area, if self.error.is_some() { 12 } else { 11 });
        self.rect = Some(overlay_area);
        let block = Block::default()
            .title(texts::HOME_CHANGE_PASSWORD)
            .borders(Borders::ALL)
            .style(ElementStyle::HomeChangePwBlock.style());

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        for (index, (label, field)) in LABELS.iter().zip(self.fields.fields()).enumerate() {
            lines.push(Line::from(password_field_spans(
                label,
                field,
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

        let inner = block.inner(overlay_area);
        let visible = LABELS.len().min(inner.height.saturating_sub(1) as usize);
        for index in 0..visible {
            let rect = Rect::new(inner.x, inner.y + 1 + index as u16, inner.width, 1);
            self.hits.push(rect, MouseButton::Left, index);
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

    #[test]
    fn a_click_focuses_the_clicked_field() {
        let mut prompt = ChangePasswordPrompt::new();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| prompt.render(frame, frame.area()))
            .unwrap();

        // The three field rows sit at inner.y + 1 + i.
        let (x, y) = (0..24u16)
            .flat_map(|y| (0..80u16).map(move |x| (x, y)))
            .find(|&(x, y)| prompt.hits.hit(x, y, MouseButton::Left) == Some(1))
            .expect("the second field must be clickable");

        prompt.handle_click(MouseButton::Left, x, y);

        assert_eq!(prompt.fields.focused_index(), 1);
    }
}
