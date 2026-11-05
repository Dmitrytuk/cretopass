//! The password pair a share bundle is encrypted with.
//!
//! Two masked fields over the shared [`crate::cursor`] widget, with the same
//! validation the create-secret prompt uses: an empty pair means a passwordless
//! bundle (which the screen must confirm), a mismatched pair is rejected, and
//! anything else is usable. The screen drives the keys and renders the fields;
//! this type only edits and validates what was typed.
//!
//! The typed password is handed out as a [`SharePassword`] — a zeroizing,
//! self-redacting buffer — so it never travels through the screens as a bare
//! `String` that outlives the prompt.

use zeroize::Zeroizing;

use crate::cursor::{FieldEdit, PairVerdict, SecretFieldGroup};
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

/// The password a share bundle is encrypted with.
///
/// The bytes live in a [`Zeroizing`] buffer, so they are wiped when the request
/// carrying them is consumed, and they are redacted from [`Debug`] so a stray
/// `{:?}` cannot print the recipient's password.
#[derive(Default)]
pub struct SharePassword(Zeroizing<Vec<u8>>);

impl SharePassword {
    /// An empty password, for the passwordless bundle a screen has confirmed.
    pub fn empty() -> Self {
        Self(Zeroizing::new(Vec::new()))
    }

    /// The password bytes, for the encryptor.
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl std::fmt::Debug for SharePassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SharePassword").field(&"<redacted>").finish()
    }
}

/// The password and its confirmation for a share bundle.
pub struct Passphrase {
    fields: SecretFieldGroup,
    error: Option<String>,
}

impl std::fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The fields redact their own text.
        f.debug_struct("Passphrase")
            .field("fields", &self.fields)
            .field("error", &self.error)
            .finish()
    }
}

impl Passphrase {
    /// A pair of empty fields, the first one focused, text hidden.
    pub fn new() -> Self {
        Self {
            fields: SecretFieldGroup::with_fields(2),
            error: None,
        }
    }

    /// The typed password (the first field), wrapped for zeroization.
    pub fn password(&self) -> SharePassword {
        SharePassword(Zeroizing::new(
            self.fields.field(0).text().as_bytes().to_vec(),
        ))
    }

    /// The rejection message, if the last confirmation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The masked fields, for the screen's renderer.
    pub(crate) fn fields(&self) -> &SecretFieldGroup {
        &self.fields
    }

    /// Focuses the field at `index`, ignoring an out-of-range index.
    ///
    /// Used by the screens to turn a click on a field row into a focus change.
    pub fn focus(&mut self, index: usize) {
        self.fields.focus(index);
    }

    /// Applies a dispatched key that edits the fields or moves between them.
    ///
    /// Cancel and Confirm are the screen's to interpret, so they are ignored
    /// here; every other [`Action`] either edits the focused field or does
    /// nothing.
    pub fn apply(&mut self, action: Action) {
        if matches!(
            self.fields.apply_action(action),
            FieldEdit::Inserted | FieldEdit::Deleted
        ) {
            self.error = None;
        }
    }

    /// Pastes clipboard `text` into the focused field, single-line only,
    /// clearing a previous rejection the way typing does.
    pub fn paste(&mut self, text: &str) {
        if self.fields.paste_clipboard(text) {
            self.error = None;
        }
    }

    /// Validates the pair and reports what it means.
    ///
    /// An empty pair is [`PassphraseSubmit::Empty`]; a mismatched pair is
    /// rejected (the report is kept and the cursors reset so it can be
    /// corrected); anything else is [`PassphraseSubmit::Password`].
    pub fn submit(&mut self) -> PassphraseSubmit {
        match self.fields.pair_verdict() {
            Some(PairVerdict::Empty) => PassphraseSubmit::Empty,
            // `None` means the group is not a pair; rejecting is the safe fallback.
            Some(PairVerdict::Mismatch) | None => {
                self.error = Some(texts::MSG_PW_MISMATCH.to_string());
                PassphraseSubmit::Mismatch
            }
            Some(PairVerdict::Match) => PassphraseSubmit::Password,
        }
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
        assert_eq!(passphrase.password().as_bytes(), b"pw");
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
    fn paste_edits_the_focused_field_and_clears_the_report() {
        let mut passphrase = Passphrase::new();
        typed(&mut passphrase, "pw", "other");
        passphrase.submit();
        assert!(passphrase.error().is_some());

        passphrase.paste("fix");

        assert_eq!(passphrase.fields().field(0).text(), "fixpw");
        assert!(passphrase.error().is_none());
    }

    #[test]
    fn an_empty_paste_is_a_no_op() {
        let mut passphrase = Passphrase::new();
        passphrase.paste("");

        assert_eq!(passphrase.fields().field(0).text(), "");
        assert_eq!(passphrase.fields().focused_index(), 0);
    }

    #[test]
    fn cancel_and_confirm_do_not_edit_the_fields() {
        let mut passphrase = Passphrase::new();
        typed(&mut passphrase, "pw", "pw");

        passphrase.apply(Action::Cancel);
        passphrase.apply(Action::Confirm);

        assert_eq!(passphrase.password().as_bytes(), b"pw");
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
    fn debug_redacts_the_password() {
        let mut passphrase = Passphrase::new();
        typed(&mut passphrase, "hunter2", "hunter2");

        assert!(
            !format!("{passphrase:?}").contains("hunter2"),
            "the pair must not print the password"
        );
        assert!(
            !format!("{:?}", passphrase.password()).contains("hunter2"),
            "the password must redact itself"
        );
    }

    #[test]
    fn unknown_actions_are_ignored() {
        let mut passphrase = Passphrase::new();
        passphrase.apply(Action::InsertChar('x'));
        passphrase.apply(Action::None);
        passphrase.apply(Action::Quit);

        assert_eq!(passphrase.password().as_bytes(), b"x");
    }
}
