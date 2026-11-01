//! The share flow's password step, with the inline passwordless confirmation.
//!
//! The pair is validated by the UI-agnostic [`crate::share::Passphrase`]. An
//! empty pair is not submitted straight away: the modal switches to a small
//! confirm screen (`y`/Enter proceeds, any other key returns to the fields),
//! mirroring the Detail modal's field-delete confirmation.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use super::chrome::centered_rect;
use crate::cursor::password_field_spans;
use crate::share::{Passphrase, PassphraseSubmit, SharePassword};
use crate::theming::{Action, ElementStyle, texts};
use crate::tree::node::Node;
use crate::tree::widget::TreeState;

/// The Share modal's password step.
#[derive(Debug)]
pub(crate) struct SharePassphrase {
    /// The Node chosen before the destination was named.
    node: Node,
    /// Whether the bundle includes `node`'s children.
    include_children: bool,
    /// The bundle folder named in the previous step.
    dest: String,
    /// The masked password pair, validated by the shared widget.
    passphrase: Passphrase,
    /// Whether the "share without a password?" confirmation is showing.
    confirming_empty: bool,
}

impl SharePassphrase {
    /// A password step for sharing `node`, with or without its children, into
    /// `dest`.
    pub(crate) fn new(node: Node, include_children: bool, dest: String) -> Self {
        Self {
            node,
            include_children,
            dest,
            passphrase: Passphrase::new(),
            confirming_empty: false,
        }
    }

    /// Reads a key: [`TreeAction::Close`] on `Esc`, [`TreeAction::Share`] once a
    /// usable (or explicitly confirmed-empty) pair is typed.
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        if self.confirming_empty {
            return match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                    Some(self.action(SharePassword::empty()))
                }
                _ => {
                    self.confirming_empty = false;
                    None
                }
            };
        }

        match Action::from_create_secret_file(&key) {
            Action::Cancel => Some(TreeAction::Close),
            Action::Confirm => match self.passphrase.submit() {
                PassphraseSubmit::Password => Some(self.action(self.passphrase.password())),
                PassphraseSubmit::Empty => {
                    self.confirming_empty = true;
                    None
                }
                // The mismatch is shown in place; the prompt stays open.
                PassphraseSubmit::Mismatch => None,
            },
            action => {
                self.passphrase.apply(action);
                None
            }
        }
    }

    /// The share request for the typed `passphrase`.
    fn action(&self, passphrase: SharePassword) -> TreeAction {
        TreeAction::Share {
            node: self.node.clone(),
            include_children: self.include_children,
            dest: self.dest.clone(),
            passphrase,
        }
    }

    /// Draws the password step, or the passwordless confirmation over it.
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        if self.confirming_empty {
            self.render_empty_confirm(frame, area);
            return;
        }

        let height = if self.passphrase.error().is_some() {
            10
        } else {
            9
        };
        let overlay_area = centered_rect(area, area.width.saturating_mul(3) / 5, height);
        let block = Block::default()
            .title(texts::SHARE_TITLE_PASSPHRASE)
            .borders(Borders::ALL)
            .style(ElementStyle::FieldTypeBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("{} {}", texts::LABEL_FILE, self.dest),
                ElementStyle::CursorLabel.style(),
            )),
            Line::from(""),
        ];

        for (index, label) in [
            texts::LABEL_PASSPHRASE_FIELD,
            texts::LABEL_PASSPHRASE_CONFIRM,
        ]
        .iter()
        .enumerate()
        {
            lines.push(Line::from(password_field_spans(
                label,
                self.passphrase.fields().field(index),
                index == self.passphrase.fields().focused_index(),
                self.passphrase.fields().show(),
            )));
        }

        if let Some(message) = self.passphrase.error() {
            lines.push(Line::from(Span::styled(
                format!(" {}", message),
                ElementStyle::AlertErrorText.style(),
            )));
        }

        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Draws the "share without a password?" confirmation.
    fn render_empty_confirm(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = centered_rect(area, area.width.saturating_mul(2) / 3, 4);
        let block = Block::default()
            .title(texts::OVERLAY_SHARE_CONFIRM)
            .borders(Borders::ALL)
            .style(ElementStyle::ConfirmDeleteBlock.style());

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!(" {}", texts::PROMPT_SHARE_NO_PASSPHRASE),
                ElementStyle::ConfirmMessage.style(),
            )),
        ];

        let inner = block.inner(overlay_area);
        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Badge and hint for the tree status bar while the password step is open.
    pub(crate) fn status_parts(&self) -> (&'static str, ElementStyle, &'static str) {
        if self.confirming_empty {
            (
                texts::MODE_SHARE_CONFIRM,
                ElementStyle::ModeConfirmDelete,
                texts::HINT_SHARE_NO_PASSPHRASE,
            )
        } else {
            (
                texts::MODE_PASSPHRASE,
                ElementStyle::ModeEdit,
                texts::HINT_SHARE_PASSPHRASE,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn prompt() -> (SharePassphrase, TreeState) {
        (
            SharePassphrase::new(Node::new("Work"), true, "/tmp/vault_Work".to_string()),
            TreeState::new(Node::new("Root")),
        )
    }

    fn typed(prompt: &mut SharePassphrase, state: &mut TreeState, password: &str, confirm: &str) {
        for c in password.chars() {
            let _ = prompt.handle_key(key(KeyCode::Char(c)), state);
        }
        let _ = prompt.handle_key(key(KeyCode::Tab), state);
        for c in confirm.chars() {
            let _ = prompt.handle_key(key(KeyCode::Char(c)), state);
        }
    }

    #[test]
    fn a_matching_pair_asks_to_share() {
        let (mut prompt, mut state) = prompt();
        typed(&mut prompt, &mut state, "pw", "pw");

        match prompt.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::Share {
                node,
                include_children,
                dest,
                passphrase,
            }) => {
                assert_eq!(node.name, "Work");
                assert!(include_children);
                assert_eq!(dest, "/tmp/vault_Work");
                assert_eq!(passphrase.as_bytes(), b"pw");
            }
            _ => panic!("expected a share action"),
        }
    }

    #[test]
    fn an_empty_pair_asks_before_sharing_without_a_password() {
        let (mut prompt, mut state) = prompt();

        assert!(prompt.handle_key(key(KeyCode::Enter), &mut state).is_none());
        assert!(prompt.confirming_empty);

        // Any other key returns to the fields.
        assert!(prompt.handle_key(key(KeyCode::Esc), &mut state).is_none());
        assert!(!prompt.confirming_empty);
    }

    #[test]
    fn confirming_the_empty_pair_shares_with_an_empty_password() {
        let (mut prompt, mut state) = prompt();
        let _ = prompt.handle_key(key(KeyCode::Enter), &mut state);

        match prompt.handle_key(key(KeyCode::Char('y')), &mut state) {
            Some(TreeAction::Share { passphrase, .. }) => assert!(passphrase.as_bytes().is_empty()),
            _ => panic!("expected a share action"),
        }
    }

    #[test]
    fn a_mismatched_pair_re_shows_the_prompt() {
        let (mut prompt, mut state) = prompt();
        typed(&mut prompt, &mut state, "pw", "other");

        assert!(prompt.handle_key(key(KeyCode::Enter), &mut state).is_none());
        assert_eq!(prompt.passphrase.error(), Some(texts::MSG_PW_MISMATCH));
    }

    #[test]
    fn esc_closes_the_prompt() {
        let (mut prompt, mut state) = prompt();
        assert!(matches!(
            prompt.handle_key(key(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn status_parts_follow_the_confirmation() {
        let (mut prompt, mut state) = prompt();
        assert_eq!(prompt.status_parts().0, texts::MODE_PASSPHRASE);
        let _ = prompt.handle_key(key(KeyCode::Enter), &mut state);
        assert_eq!(prompt.status_parts().0, texts::MODE_SHARE_CONFIRM);
    }
}
