//! The share flow's password step: the recipient's password, twice.
//!
//! The pair is validated by the UI-agnostic [`crate::share::Passphrase`]; this
//! modal only renders it with the Home chrome and reports what the user asked
//! for. An empty pair is reported as [`HomeAction::ShareNoPassphrase`] so the
//! orchestrator can ask for confirmation first, exactly like creating a
//! passwordless secret file.

use std::borrow::Cow;

use crossterm::event::{KeyEvent, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::HomeAction;
use crate::cursor::password_field_spans;
use crate::hitbox::{HitMap, is_left};
use crate::share::{Passphrase, PassphraseSubmit};
use crate::theming::{Action, ElementStyle, texts};
use crate::vault::widget::chrome::{display_path, make_overlay, paint_overlay};

/// The labels of the two fields, in focus order.
const LABELS: [&str; 2] = [
    texts::LABEL_PASSPHRASE_FIELD,
    texts::LABEL_PASSPHRASE_CONFIRM,
];

/// The dialog that types the password a share bundle is encrypted with.
pub struct SharePassphrase {
    /// The bundle folder, shown so the user knows what is about to be written.
    dest: String,
    passphrase: Passphrase,
    /// The clickable field rows recorded by the renderer.
    hits: HitMap<usize>,
}

impl SharePassphrase {
    /// A dialog for the bundle at `dest`, with both fields empty.
    pub fn new(dest: String) -> Self {
        Self {
            dest,
            passphrase: Passphrase::new(),
            hits: HitMap::default(),
        }
    }

    /// `Tab` moves between the two fields, `Ctrl+S` reveals both, `Esc` dismisses,
    /// and `Enter` reports the pair.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match Action::from_create_secret_file(&key) {
            Action::Cancel => Some(HomeAction::Close),
            Action::Confirm => match self.passphrase.submit() {
                PassphraseSubmit::Password => Some(HomeAction::Share {
                    dest: self.dest.clone(),
                    passphrase: self.passphrase.password(),
                }),
                PassphraseSubmit::Empty => Some(HomeAction::ShareNoPassphrase {
                    dest: self.dest.clone(),
                }),
                // The mismatch is shown in place; the prompt stays open.
                PassphraseSubmit::Mismatch => None,
            },
            action => {
                self.passphrase.apply(action);
                None
            }
        }
    }

    /// Pastes the clipboard's text into the focused password field,
    /// single-line only.
    pub fn handle_paste(&mut self, text: &str) {
        self.passphrase.paste(text);
    }

    /// A left click on a field row focuses that field; the right button is
    /// ignored.
    pub fn handle_click(&mut self, button: MouseButton, x: u16, y: u16) {
        if !is_left(button) {
            return;
        }
        if let Some(index) = self.hits.hit(x, y, button) {
            self.passphrase.focus(index);
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, display_full_path: bool) {
        self.hits.clear();
        let shown_dest = display_path(&self.dest, display_full_path);
        let height = if self.passphrase.error().is_some() {
            10
        } else {
            9
        };
        let overlay_area = make_overlay(area, height);
        let block = Block::default()
            .title(texts::SHARE_TITLE_PASSPHRASE)
            .borders(Borders::ALL)
            .style(ElementStyle::HomePassBlock.style());

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("{} {}", texts::LABEL_FILE, shown_dest),
            ElementStyle::CursorLabel.style(),
        )));
        lines.push(Line::from(""));

        for (index, (label, field)) in LABELS
            .iter()
            .zip(self.passphrase.fields().fields())
            .enumerate()
        {
            lines.push(Line::from(password_field_spans(
                label,
                field,
                index == self.passphrase.fields().focused_index(),
                self.passphrase.fields().show(),
            )));
        }

        if let Some(message) = self.passphrase.error() {
            lines.push(Line::from(Span::styled(
                format!(" {}", message),
                ElementStyle::HomeErrorBar.style(),
            )));
        }

        lines.push(Line::from(""));

        let inner = block.inner(overlay_area);
        let visible = LABELS.len().min(inner.height.saturating_sub(3) as usize);
        for index in 0..visible {
            let rect = Rect::new(inner.x, inner.y + 3 + index as u16, inner.width, 1);
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
            texts::MODE_PASSPHRASE,
            ElementStyle::ModeEdit,
            Cow::Borrowed(texts::HINT_SHARE_PASSPHRASE),
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

    fn typed(prompt: &mut SharePassphrase, password: &str, confirmation: &str) {
        for c in password.chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }
        prompt.handle_key(key(KeyCode::Tab));
        for c in confirmation.chars() {
            prompt.handle_key(key(KeyCode::Char(c)));
        }
        prompt.handle_key(key(KeyCode::Tab));
    }

    #[test]
    fn a_matching_pair_asks_to_share() {
        let mut prompt = SharePassphrase::new("/tmp/bundle".to_string());
        typed(&mut prompt, "pw", "pw");

        assert!(matches!(
            prompt.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::Share { dest, passphrase })
                if dest == "/tmp/bundle" && passphrase.as_bytes() == b"pw"
        ));
    }

    #[test]
    fn an_empty_pair_asks_about_a_passwordless_bundle() {
        let mut prompt = SharePassphrase::new("/tmp/bundle".to_string());

        assert!(matches!(
            prompt.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::ShareNoPassphrase { dest }) if dest == "/tmp/bundle"
        ));
    }

    #[test]
    fn a_mismatched_pair_re_shows_the_dialog() {
        let mut prompt = SharePassphrase::new("/tmp/bundle".to_string());
        typed(&mut prompt, "pw", "other");

        assert!(prompt.handle_key(key(KeyCode::Enter)).is_none());
        assert_eq!(prompt.passphrase.error(), Some(texts::MSG_PW_MISMATCH));
    }

    #[test]
    fn esc_dismisses_the_dialog() {
        let mut prompt = SharePassphrase::new("/tmp/bundle".to_string());
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn status_parts_advertise_the_passphrase_step() {
        let prompt = SharePassphrase::new("/tmp/bundle".to_string());
        let (mode, _, hint) = prompt.status_parts();
        assert_eq!(mode, texts::MODE_PASSPHRASE);
        assert_eq!(hint.as_ref(), texts::HINT_SHARE_PASSPHRASE);
    }
}
