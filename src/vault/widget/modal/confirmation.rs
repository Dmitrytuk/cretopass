//! The yes/no confirmation.
//!
//! Three Home flows ask the user to confirm before something surprising or
//! irreversible: dropping a recent secret file, re-encrypting the open file with
//! a new master password, and creating one without a password at all. They differ
//! only in prompt and in what "yes" means, so they are one modal with a
//! [`ConfirmKind`] discriminant.
//!
//! The prompt, block style and status-bar vocabulary all follow from the kind, so
//! a call site cannot make them drift apart.

use std::borrow::Cow;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::HomeAction;
use crate::share::SharePassword;
use crate::theming::{Action, ElementStyle, texts};
use crate::vault::widget::chrome::{make_overlay, paint_overlay};

/// Which confirmation is being asked, and the data its prompt needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    /// Drop a secret file from the recent list.
    DeleteRecent { path: String },
    /// Re-encrypt the open secret file with a new master password.
    ChangePassword,
    /// Create a secret file with no master password.
    NoPassphrase { file_path: String },
    /// Share the open file with no password at all.
    ShareNoPassphrase { dest: String },
}

/// A confirmation dialog: `y`/`Enter` accepts, anything else declines.
pub struct Confirmation {
    kind: ConfirmKind,
}

impl Confirmation {
    pub fn delete_recent(path: String) -> Self {
        Self {
            kind: ConfirmKind::DeleteRecent { path },
        }
    }

    pub fn change_password() -> Self {
        Self {
            kind: ConfirmKind::ChangePassword,
        }
    }

    pub fn no_passphrase(file_path: String) -> Self {
        Self {
            kind: ConfirmKind::NoPassphrase { file_path },
        }
    }

    /// A confirmation that the share bundle at `dest` will have no password.
    pub fn share_no_passphrase(dest: String) -> Self {
        Self {
            kind: ConfirmKind::ShareNoPassphrase { dest },
        }
    }

    /// `y`/`Enter` accepts; every other key declines, as the hint promises.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match Action::from_confirm(&key) {
            Action::Yes => Some(self.accept()),
            _ => Some(self.decline()),
        }
    }

    /// What accepting this confirmation asks for.
    fn accept(&self) -> HomeAction {
        match &self.kind {
            ConfirmKind::DeleteRecent { path } => HomeAction::DeleteRecent { path: path.clone() },
            ConfirmKind::ChangePassword => HomeAction::ChangePasswordPrompt,
            ConfirmKind::NoPassphrase { file_path } => HomeAction::CreateWithoutPassword {
                file_path: file_path.clone(),
            },
            ConfirmKind::ShareNoPassphrase { dest } => HomeAction::Share {
                dest: dest.clone(),
                passphrase: SharePassword::empty(),
            },
        }
    }

    /// What declining this confirmation asks for.
    ///
    /// Declining to create a passwordless file is not a cancel: the user goes
    /// back to the create dialog to type one.
    fn decline(&self) -> HomeAction {
        match &self.kind {
            ConfirmKind::NoPassphrase { file_path } => HomeAction::ReopenCreateSecret {
                file_path: file_path.clone(),
            },
            ConfirmKind::ShareNoPassphrase { dest } => {
                HomeAction::ReopenSharePassphrase { dest: dest.clone() }
            }
            _ => HomeAction::Close,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let title = match &self.kind {
            ConfirmKind::DeleteRecent { .. } => texts::HOME_DELETE_CONFIRM,
            ConfirmKind::ChangePassword => texts::HOME_CHANGE_PASSWORD,
            ConfirmKind::NoPassphrase { .. } | ConfirmKind::ShareNoPassphrase { .. } => {
                texts::HOME_NO_PASSPHRASE
            }
        };
        let style = match &self.kind {
            ConfirmKind::DeleteRecent { .. } => ElementStyle::HomeDeleteBlock,
            ConfirmKind::ChangePassword => ElementStyle::HomeChangePwBlock,
            ConfirmKind::NoPassphrase { .. } | ConfirmKind::ShareNoPassphrase { .. } => {
                ElementStyle::HomeNoPassBlock
            }
        };

        let overlay_area = make_overlay(area, 6);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .style(style.style());

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                self.prompt(),
                ElementStyle::ConfirmMessage.style(),
            )),
            Line::from(""),
        ];

        paint_overlay(
            frame,
            overlay_area,
            block,
            Paragraph::new(Text::from(lines)),
        );
    }

    /// The question shown in the dialog.
    fn prompt(&self) -> String {
        match &self.kind {
            ConfirmKind::DeleteRecent { path } => format!(
                "{}{}{}",
                texts::PROMPT_DELETE_RECENT_START,
                path,
                texts::PROMPT_DELETE_RECENT_END
            ),
            ConfirmKind::ChangePassword => texts::PROMPT_CHANGE_PW.to_string(),
            ConfirmKind::NoPassphrase { .. } => texts::PROMPT_NO_PASSPHRASE.to_string(),
            ConfirmKind::ShareNoPassphrase { .. } => texts::PROMPT_SHARE_NO_PASSPHRASE.to_string(),
        }
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        let (mode, hint) = match &self.kind {
            ConfirmKind::DeleteRecent { .. } => (texts::MODE_DELETE, texts::HINT_DELETE_CONFIRM),
            ConfirmKind::ChangePassword => (texts::MODE_CHANGE_PW, texts::HINT_CHANGE_PW_CONFIRM),
            ConfirmKind::NoPassphrase { .. } => (texts::MODE_CONFIRM, texts::HINT_NO_PASSPHRASE),
            ConfirmKind::ShareNoPassphrase { .. } => {
                (texts::MODE_CONFIRM, texts::HINT_SHARE_NO_PASSPHRASE)
            }
        };
        (mode, ElementStyle::ModeConfirmDelete, Cow::Borrowed(hint))
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
    fn accepting_delete_drops_that_recent_file() {
        let mut confirmation = Confirmation::delete_recent("/tmp/a.age".to_string());
        let action = confirmation.handle_key(key(KeyCode::Enter));
        assert!(matches!(action, Some(HomeAction::DeleteRecent { path }) if path == "/tmp/a.age"));
    }

    #[test]
    fn declining_delete_closes_the_dialog() {
        let mut confirmation = Confirmation::delete_recent("/tmp/a.age".to_string());
        assert!(matches!(
            confirmation.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn any_other_key_declines() {
        let mut confirmation = Confirmation::change_password();
        assert!(matches!(
            confirmation.handle_key(key(KeyCode::Char('x'))),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn declining_the_passwordless_create_returns_to_the_create_dialog() {
        let mut confirmation = Confirmation::no_passphrase("/tmp/new.age".to_string());
        assert!(matches!(
            confirmation.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::ReopenCreateSecret { file_path }) if file_path == "/tmp/new.age"
        ));
    }

    #[test]
    fn accepting_the_passwordless_create_asks_for_the_file() {
        let mut confirmation = Confirmation::no_passphrase("/tmp/new.age".to_string());
        assert!(matches!(
            confirmation.handle_key(key(KeyCode::Char('y'))),
            Some(HomeAction::CreateWithoutPassword { file_path }) if file_path == "/tmp/new.age"
        ));
    }

    #[test]
    fn each_kind_keeps_its_own_status_vocabulary() {
        let delete = Confirmation::delete_recent("/tmp/a.age".to_string());
        let (mode, _, hint) = delete.status_parts();
        assert_eq!(mode, texts::MODE_DELETE);
        assert_eq!(hint.as_ref(), texts::HINT_DELETE_CONFIRM);

        let change = Confirmation::change_password();
        let (mode, _, hint) = change.status_parts();
        assert_eq!(mode, texts::MODE_CHANGE_PW);
        assert_eq!(hint.as_ref(), texts::HINT_CHANGE_PW_CONFIRM);

        let no_pass = Confirmation::no_passphrase("/tmp/a.age".to_string());
        let (mode, _, hint) = no_pass.status_parts();
        assert_eq!(mode, texts::MODE_CONFIRM);
        assert_eq!(hint.as_ref(), texts::HINT_NO_PASSPHRASE);
    }

    #[test]
    fn accepting_the_passwordless_share_writes_an_empty_password() {
        let mut confirmation = Confirmation::share_no_passphrase("/tmp/bundle".to_string());
        assert!(matches!(
            confirmation.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::Share { dest, passphrase })
                if dest == "/tmp/bundle" && passphrase.is_empty()
        ));
    }

    #[test]
    fn declining_the_passwordless_share_returns_to_the_prompt() {
        let mut confirmation = Confirmation::share_no_passphrase("/tmp/bundle".to_string());
        assert!(matches!(
            confirmation.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::ReopenSharePassphrase { dest }) if dest == "/tmp/bundle"
        ));
    }
}
