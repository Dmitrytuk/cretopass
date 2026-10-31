//! The password pair a share bundle is encrypted with.
//!
//! Two masked fields over the shared [`cursor`] widget, with the same
//! validation the create-secret prompt uses: an empty pair means a passwordless
//! bundle (which the screen must confirm), a mismatched pair is rejected, and
//! anything else is usable. The screen drives the keys and renders the fields;
//! this type only edits and validates what was typed.

use crate::cursor::{SecretField, SecretFieldGroup};
use crate::theming::{Action, texts};

/// What confirming the password pair produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassphraseSubmit {
    /// Both fields match and hold a non-empty password; read it with
    /// [`Passphrase::password`].
    Password,
    /// Both fields are empty: a passwordless bundle the screen must confirm
    /// before sharing.
    Empty,
    /// The two fields differ; the pair was rejected and kept for correcting.
    Mismatch,
}

/// The password and its confirmation for a share bundle.
#[derive(Debug, Clone)]
pub struct Passphrase {
    fields: SecretFieldGroup,
    error: Option<String>,
}

impl Passphrase {
    /// A pair of empty fields, the first one focused, text hidden.
    pub fn new() -> Self {
        Self {
            fields: SecretFieldGroup::with_fields(2),
            error: None,
        }
    }

    /// The typed password (the first field).
    pub fn password(&self) -> String {
        self.fields.field(0).text().to_string()
    }

    /// The rejection message, if the last confirmation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The masked fields, for the screen's renderer.
    pub fn fields(&self) -> &SecretFieldGroup {
        &self.fields
    }

    /// Applies a dispatched key that edits the fields or moves between them.
    ///
    /// Cancel and Confirm are the screen's to interpret, so they are ignored
    /// here; every other [`Action`] either edits the focused field or does
    /// nothing.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::NextField => self.fields.next_field(),
            Action::PrevField => self.fields.prev_field(),
            Action::ToggleShowPassword => self.fields.toggle_show(),
            Action::InsertChar(c) => self.edit(|field| field.insert(c)),
            Action::Backspace => self.edit(SecretField::backspace),
            Action::DeleteChar => self.edit(SecretField::delete),
            Action::CursorLeft => self.fields.apply(SecretField::move_left),
            Action::CursorRight => self.fields.apply(SecretField::move_right),
            Action::CursorHome => self.fields.apply(SecretField::move_home),
            Action::CursorEnd => self.fields.apply(SecretField::move_end),
            _ => {}
        }
    }

    /// Validates the pair and reports what it means.
    ///
    /// An empty pair is [`PassphraseSubmit::Empty`]; a mismatched pair is
    /// rejected (the report is kept and the cursors reset so it can be
    /// corrected); anything else is [`PassphraseSubmit::Password`].
    pub fn submit(&mut self) -> PassphraseSubmit {
        let password = self.fields.field(0).text();
        let confirmation = self.fields.field(1).text();

        if password.is_empty() && confirmation.is_empty() {
            return PassphraseSubmit::Empty;
        }
        if password != confirmation {
            self.error = Some(texts::MSG_PW_MISMATCH.to_string());
            self.fields.reset_cursors();
            return PassphraseSubmit::Mismatch;
        }
        PassphraseSubmit::Password
    }

    /// Edits the focused field and clears the previous report.
    fn edit(&mut self, action: impl FnOnce(&mut SecretField)) {
        self.fields.apply(action);
        self.error = None;
    }
}

impl Default for Passphrase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `text` into the focused field, switching fields between the two
    /// arguments.
    fn typed(passphrase: &mut Passphrase, password: &str, confirmation: &str) {
        for c in password.chars() {
            passphrase.apply(Action::InsertChar(c));
        }
        passphrase.apply(Action::NextField);
        for c in confirmation.chars() {
            passphrase.apply(Action::InsertChar(c));
        }
    }

    #[test]
    fn a_matching_pair_is_usable() {
        let mut passphrase = Passphrase::new();
        typed(&mut passphrase, "pw", "pw");

        assert_eq!(passphrase.submit(), PassphraseSubmit::Password);
        assert_eq!(passphrase.password(), "pw");
        assert!(passphrase.error().is_none());
    }

    #[test]
    fn an_empty_pair_asks_for_confirmation() {
        let mut passphrase = Passphrase::new();

        assert_eq!(passphrase.submit(), PassphraseSubmit::Empty);
        assert!(passphrase.error().is_none());
    }

    #[test]
    fn a_mismatched_pair_is_rejected_and_kept() {
        let mut passphrase = Passphrase::new();
        typed(&mut passphrase, "pw", "other");

        assert_eq!(passphrase.submit(), PassphraseSubmit::Mismatch);
        assert_eq!(passphrase.error(), Some(texts::MSG_PW_MISMATCH));
        assert_eq!(passphrase.fields().field(0).text(), "pw");
        assert_eq!(
            passphrase.fields().field(0).cursor(),
            0,
            "cursors are reset"
        );
    }

    #[test]
    fn editing_clears_the_previous_report() {
        let mut passphrase = Passphrase::new();
        typed(&mut passphrase, "pw", "other");
        passphrase.submit();
        assert!(passphrase.error().is_some());

        passphrase.apply(Action::Backspace);

        assert!(passphrase.error().is_none());
    }

    #[test]
    fn cancel_and_confirm_do_not_edit_the_fields() {
        let mut passphrase = Passphrase::new();
        typed(&mut passphrase, "pw", "pw");

        passphrase.apply(Action::Cancel);
        passphrase.apply(Action::Confirm);

        assert_eq!(passphrase.password(), "pw");
    }

    #[test]
    fn tab_switches_fields_and_ctrl_s_reveals() {
        let mut passphrase = Passphrase::new();
        passphrase.apply(Action::NextField);
        assert_eq!(passphrase.fields().focused_index(), 1);
        passphrase.apply(Action::PrevField);
        assert_eq!(passphrase.fields().focused_index(), 0);

        assert!(!passphrase.fields().show());
        passphrase.apply(Action::ToggleShowPassword);
        assert!(passphrase.fields().show());
    }

    #[test]
    fn unknown_actions_are_ignored() {
        let mut passphrase = Passphrase::new();
        passphrase.apply(Action::InsertChar('x'));
        passphrase.apply(Action::None);
        passphrase.apply(Action::Quit);

        assert_eq!(passphrase.password(), "x");
    }
}
