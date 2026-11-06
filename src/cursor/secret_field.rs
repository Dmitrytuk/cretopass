//! Masked password fields shared by the Home prompts and the Share flow.
//!
//! The unlock, create-secret and change-password prompts all edit N masked text
//! fields with one focused field and a shared "reveal plaintext" flag. This
//! module owns that behaviour on top of the [`super::editing`] editing
//! primitives, so every prompt stops re-implementing the same editing and
//! span-building code.

use ratatui::text::Span;
use zeroize::{Zeroize, Zeroizing};

use super::editing::{
    PastePolicy, backspace_at, cursor_left, cursor_right, delete_at, insert_char_at, insert_str_at,
    split_at_cursor,
};
use crate::theming::{Action, ElementStyle};

/// A single masked text field: its text and a byte-offset cursor
/// ([ADR 0003](docs/adr/0003-cursor.md)).
///
/// The text is a plaintext secret: it is wiped from the heap when the field is
/// dropped ([`Zeroize`]) and redacted from [`Debug`], mirroring `SaveTarget`.
#[derive(Default, Clone)]
pub struct SecretField {
    text: String,
    cursor: usize,
}

impl std::fmt::Debug for SecretField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretField")
            .field("text", &"<redacted>")
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl Drop for SecretField {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

impl SecretField {
    /// The field's text, always masked at render time unless revealed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The cursor position as a byte offset into [`SecretField::text`].
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Inserts `c` at the cursor and advances past it.
    pub fn insert(&mut self, c: char) {
        insert_char_at(&mut self.text, &mut self.cursor, c);
    }

    /// Inserts the whole string `text` at the cursor and advances past it.
    ///
    /// `text` is expected to be already sanitised by the caller's
    /// [`PastePolicy`]; the field itself only places it.
    pub fn paste(&mut self, text: &str) {
        insert_str_at(&mut self.text, &mut self.cursor, text);
    }

    /// Removes the character before the cursor; a no-op at the start.
    pub fn backspace(&mut self) {
        self.cursor = backspace_at(&mut self.text, self.cursor);
    }

    /// Removes the character under the cursor; a no-op at the end.
    pub fn delete(&mut self) {
        delete_at(&mut self.text, self.cursor);
    }

    /// Moves the cursor one character to the left.
    pub fn move_left(&mut self) {
        self.cursor = cursor_left(&self.text, self.cursor);
    }

    /// Moves the cursor one character to the right.
    pub fn move_right(&mut self) {
        self.cursor = cursor_right(&self.text, self.cursor);
    }

    /// Moves the cursor to the start of the text.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Moves the cursor to the end of the text.
    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    /// Empties the field, wiping the text from the heap immediately.
    fn clear(&mut self) {
        self.text.zeroize();
        self.cursor = 0;
    }
}

/// What a [`SecretFieldGroup::apply_action`] call did to the fields.
///
/// The prompts share one key map, but not one clear-the-report policy: typing
/// again clears a rejection everywhere, while erasing or moving may not. The
/// variant lets each prompt keep its own policy without re-implementing the
/// dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldEdit {
    /// A character was inserted into the focused field.
    Inserted,
    /// A character was removed from the focused field.
    Deleted,
    /// A cursor moved, the focus switched, or the reveal flag flipped.
    Moved,
    /// The action does not touch the fields (`Confirm`, `Cancel`, …).
    Unhandled,
}

/// The verdict of validating a two-field passphrase pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairVerdict {
    /// Both fields are empty: a passwordless flow the caller must confirm.
    Empty,
    /// The two fields differ; the cursors were reset so it can be corrected.
    Mismatch,
    /// The pair matches and is not empty.
    Match,
}

/// A group of [`SecretField`]s with one focused field and one reveal flag.
///
/// The prompts differ only in how many fields they hold: one for unlocking, two
/// for creating a secret file or a share bundle, three for changing the master
/// password. Built with [`SecretFieldGroup::with_fields`]; it deliberately has
/// no `Default`, so an empty group cannot be conjured by accident.
pub struct SecretFieldGroup {
    fields: Vec<SecretField>,
    focused: usize,
    show: bool,
}

impl std::fmt::Debug for SecretFieldGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Each field redacts its own text; print the shape, not the contents.
        f.debug_struct("SecretFieldGroup")
            .field("fields", &self.fields.len())
            .field("focused", &self.focused)
            .field("show", &self.show)
            .finish()
    }
}

impl SecretFieldGroup {
    /// A group of `count` empty fields, the first one focused, text hidden.
    pub fn with_fields(count: usize) -> Self {
        Self {
            fields: vec![SecretField::default(); count],
            focused: 0,
            show: false,
        }
    }

    /// The field at `index`.
    ///
    /// # Panics
    ///
    /// Panics when `index` is out of range. Every caller passes an index within
    /// the count the group was built with
    /// ([`SecretFieldGroup::with_fields`]).
    pub fn field(&self, index: usize) -> &SecretField {
        &self.fields[index]
    }

    /// Every field, in focus order, for a renderer that zips them with its
    /// labels instead of indexing each one (see the prompt renderers).
    pub fn fields(&self) -> &[SecretField] {
        &self.fields
    }

    /// The index of the focused field.
    pub fn focused_index(&self) -> usize {
        self.focused
    }

    /// Whether the plaintext is currently revealed.
    pub fn show(&self) -> bool {
        self.show
    }

    /// Flips the reveal flag.
    pub fn toggle_show(&mut self) {
        self.show = !self.show;
    }

    /// Focuses the next field, wrapping to the first one.
    pub fn next_field(&mut self) {
        let count = self.fields.len();
        if count > 0 {
            self.focused = (self.focused + 1) % count;
        }
    }

    /// Focuses the previous field, stopping at the first one.
    pub fn prev_field(&mut self) {
        self.focused = self.focused.saturating_sub(1);
    }

    /// Focuses the field at `index`, ignoring an out-of-range index.
    ///
    /// Used by the screens to turn a click on a field row into a focus change.
    pub fn focus(&mut self, index: usize) {
        if index < self.fields.len() {
            self.focused = index;
        }
    }

    /// Runs `action` on the focused field.
    pub fn apply(&mut self, action: impl FnOnce(&mut SecretField)) {
        if let Some(field) = self.fields.get_mut(self.focused) {
            action(field);
        }
    }

    /// Inserts the whole string `text` into the focused field.
    ///
    /// Like [`SecretField::paste`], `text` is expected to be already sanitised
    /// by the caller; this only routes it to the focused field.
    pub fn paste(&mut self, text: &str) {
        self.apply(|field| field.paste(text));
    }

    /// Pastes clipboard `text` into the focused field, single-line only.
    ///
    /// A masked field is always single-line, so this is the entry point every
    /// password prompt uses. The cleaned text borrows the caller's buffer when
    /// nothing needs stripping — the common case, and the caller already holds
    /// it in a [`Zeroizing`] buffer — so no second copy of the secret is made.
    /// Only a rewritten paste allocates, and [`PastePolicy::apply_cleaned`] wipes
    /// that copy. Returns `true` when something was inserted, so a caller only
    /// clears its rejection on a real paste.
    pub fn paste_clipboard(&mut self, text: &str) -> bool {
        PastePolicy::SingleLine.apply_cleaned(text, |cleaned| {
            if cleaned.is_empty() {
                return false;
            }
            self.paste(cleaned);
            true
        })
    }

    /// Applies a dispatched prompt key to the group, reporting what it did.
    ///
    /// Every masked-password prompt — unlock, create-secret, change-password and
    /// the share passphrase — dispatches the same [`Action`] set. This is the one
    /// place that maps it onto the fields; the prompts keep only their own
    /// `Cancel`, `Confirm` and clear-the-report policy.
    pub fn apply_action(&mut self, action: Action) -> FieldEdit {
        match action {
            Action::NextField => {
                self.next_field();
                FieldEdit::Moved
            }
            Action::PrevField => {
                self.prev_field();
                FieldEdit::Moved
            }
            Action::ToggleShowPassword => {
                self.toggle_show();
                FieldEdit::Moved
            }
            Action::InsertChar(c) => {
                self.apply(|field| field.insert(c));
                FieldEdit::Inserted
            }
            Action::Backspace => {
                self.apply(SecretField::backspace);
                FieldEdit::Deleted
            }
            Action::DeleteChar => {
                self.apply(SecretField::delete);
                FieldEdit::Deleted
            }
            Action::CursorLeft => {
                self.apply(SecretField::move_left);
                FieldEdit::Moved
            }
            Action::CursorRight => {
                self.apply(SecretField::move_right);
                FieldEdit::Moved
            }
            Action::CursorHome => {
                self.apply(SecretField::move_home);
                FieldEdit::Moved
            }
            Action::CursorEnd => {
                self.apply(SecretField::move_end);
                FieldEdit::Moved
            }
            _ => FieldEdit::Unhandled,
        }
    }

    /// Validates the two-field passphrase pair (the first two fields) with the
    /// rule the create-secret and share prompts share: both fields empty is the
    /// passwordless case, a mismatch resets the cursors to be corrected, and a
    /// matching non-empty pair is usable.
    ///
    /// Returns `None` when the group does not hold a pair (fewer than two
    /// fields). Every caller builds a pair with
    /// [`with_fields(2)`](SecretFieldGroup::with_fields), so `None` is not
    /// reachable in practice; a debug assertion catches a regression.
    pub fn pair_verdict(&mut self) -> Option<PairVerdict> {
        debug_assert!(
            self.fields.len() >= 2,
            "pair_verdict needs a two-field group, got {} fields",
            self.fields.len()
        );

        let first = self.fields.first()?;
        let second = self.fields.get(1)?;
        if first.text().is_empty() && second.text().is_empty() {
            return Some(PairVerdict::Empty);
        }
        if first.text() != second.text() {
            self.reset_cursors();
            return Some(PairVerdict::Mismatch);
        }
        Some(PairVerdict::Match)
    }

    /// Moves every cursor to the start and focuses the first field, keeping the
    /// typed text and the reveal flag (the "fix your input" path).
    pub fn reset_cursors(&mut self) {
        for field in &mut self.fields {
            field.move_home();
        }
        self.focused = 0;
    }

    /// Empties every field, focuses the first one and hides the text (the "start
    /// over" path).
    pub fn clear(&mut self) {
        for field in &mut self.fields {
            field.clear();
        }
        self.focused = 0;
        self.show = false;
    }
}

/// Pastes clipboard `text` into `fields`, clearing `error` when something was
/// actually inserted.
///
/// The unlock, create-secret, change-password and share-passphrase prompts all
/// run exactly this on a paste. Keeping it here means the "clear a rejection
/// only after a real insert" rule lives once, not four times.
pub(crate) fn paste_clearing_error(
    fields: &mut SecretFieldGroup,
    error: &mut Option<String>,
    text: &str,
) {
    if fields.paste_clipboard(text) {
        *error = None;
    }
}

/// Spans for the value of `field`: masked unless `show`, with an inverted cursor
/// cell while the field is focused.
///
/// The mask is one `*` per **byte** of text, not per character
/// (`"*".repeat(field.text().len())`). That is deliberate: the cursor is a byte
/// offset, so a per-byte mask keeps the inverted cell aligned with the cursor
/// for multibyte input too — masking by character count would silently misplace
/// it. While `show` is set, the plaintext display copy is held in a
/// [`Zeroizing`] buffer so it is wiped when this function returns.
pub fn secret_field_spans(field: &SecretField, focused: bool, show: bool) -> Vec<Span<'static>> {
    let display = Zeroizing::new(if show {
        field.text().to_string()
    } else {
        "*".repeat(field.text().len())
    });

    if !focused {
        return vec![Span::styled(
            display.to_string(),
            ElementStyle::HomeFieldLabel.style(),
        )];
    }

    let (before, ch, after) = split_at_cursor(&display, field.cursor());

    let mut spans = Vec::with_capacity(3);
    if !before.is_empty() {
        spans.push(Span::raw(before.to_string()));
    }
    spans.push(Span::styled(
        ch.map_or_else(|| " ".to_string(), |c| c.to_string()),
        ElementStyle::InvertedCursor.style(),
    ));
    if !after.is_empty() {
        spans.push(Span::raw(after.to_string()));
    }
    spans
}

/// The padded `  Label: ` prefix followed by [`secret_field_spans`]: the layout
/// the multi-field prompts use.
pub fn password_field_spans(
    label: &str,
    field: &SecretField,
    focused: bool,
    show: bool,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!("  {label}: "),
        ElementStyle::HomeFieldLabel.style(),
    )];
    spans.extend(secret_field_spans(field, focused, show));
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(chars: &str) -> SecretField {
        let mut field = SecretField::default();
        for c in chars.chars() {
            field.insert(c);
        }
        field
    }

    #[test]
    fn editing_keeps_the_cursor_on_a_char_boundary() {
        let mut field = typed("aй");
        assert_eq!(field.text(), "aй");
        assert_eq!(field.cursor(), 3);

        field.move_left();
        assert_eq!(field.cursor(), 1);
        field.backspace();
        assert_eq!(field.text(), "й");
        assert_eq!(field.cursor(), 0);

        field.delete();
        assert_eq!(field.text(), "");
    }

    #[test]
    fn home_and_end_move_to_the_edges() {
        let mut field = typed("secret");
        field.move_home();
        assert_eq!(field.cursor(), 0);
        field.move_end();
        assert_eq!(field.cursor(), "secret".len());
    }

    #[test]
    fn apply_targets_only_the_focused_field() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.apply(|f| f.insert('a'));
        group.next_field();
        group.apply(|f| f.insert('b'));

        assert_eq!(group.field(0).text(), "a");
        assert_eq!(group.field(1).text(), "b");
    }

    #[test]
    fn paste_inserts_into_the_focused_field_at_the_cursor() {
        let mut field = typed("ac");
        field.move_left(); // between `a` and `c`
        field.paste("b");

        assert_eq!(field.text(), "abc");
        assert_eq!(field.cursor(), 2);
    }

    #[test]
    fn group_paste_targets_only_the_focused_field() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.paste("one");
        group.next_field();
        group.paste("two");

        assert_eq!(group.field(0).text(), "one");
        assert_eq!(group.field(1).text(), "two");
    }

    #[test]
    fn paste_clipboard_joins_lines_and_reports_a_real_paste() {
        let mut group = SecretFieldGroup::with_fields(1);

        assert!(group.paste_clipboard("pw\n"));
        assert_eq!(group.field(0).text(), "pw");

        assert!(
            !group.paste_clipboard("\n\r\n"),
            "a clipped-away paste inserts nothing"
        );
        assert_eq!(group.field(0).text(), "pw");
    }

    #[test]
    fn a_real_paste_clears_the_rejection_but_a_clipped_away_one_does_not() {
        let mut fields = SecretFieldGroup::with_fields(1);
        let mut error = Some("wrong password".to_string());

        paste_clearing_error(&mut fields, &mut error, "\n");
        assert_eq!(
            error.as_deref(),
            Some("wrong password"),
            "nothing was inserted, so the rejection stays"
        );

        paste_clearing_error(&mut fields, &mut error, "pw");
        assert!(error.is_none(), "a real paste clears the rejection");
        assert_eq!(fields.field(0).text(), "pw");
    }

    #[test]
    fn focus_wraps_forward_and_stops_backward() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.prev_field();
        assert_eq!(group.focused_index(), 0);

        group.next_field();
        assert_eq!(group.focused_index(), 1);
        group.next_field();
        assert_eq!(group.focused_index(), 0);
    }

    #[test]
    fn apply_action_reports_what_it_did() {
        let mut group = SecretFieldGroup::with_fields(2);

        assert_eq!(
            group.apply_action(Action::InsertChar('x')),
            FieldEdit::Inserted
        );
        assert_eq!(group.apply_action(Action::Backspace), FieldEdit::Deleted);
        assert_eq!(group.apply_action(Action::CursorLeft), FieldEdit::Moved);
        assert_eq!(
            group.apply_action(Action::ToggleShowPassword),
            FieldEdit::Moved
        );
        assert_eq!(group.apply_action(Action::NextField), FieldEdit::Moved);
        assert_eq!(group.apply_action(Action::Confirm), FieldEdit::Unhandled);
        assert_eq!(group.field(0).text(), "", "the char was backspaced away");
    }

    #[test]
    fn pair_verdict_matches_the_two_field_rule() {
        let mut group = SecretFieldGroup::with_fields(2);

        assert_eq!(group.pair_verdict(), Some(PairVerdict::Empty));

        group.apply(|f| f.insert('a'));
        group.next_field();
        group.apply(|f| f.insert('b'));
        assert_eq!(group.pair_verdict(), Some(PairVerdict::Mismatch));
        assert_eq!(group.focused_index(), 0, "a mismatch resets the focus");

        group.clear();
        group.apply(|f| f.insert('b'));
        group.next_field();
        group.apply(|f| f.insert('b'));
        assert_eq!(group.pair_verdict(), Some(PairVerdict::Match));
    }

    #[test]
    fn a_one_sided_empty_pair_is_a_mismatch() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.apply(|f| f.insert('a'));
        group.apply(|f| f.move_end());
        group.next_field();
        group.apply(|f| f.insert('b'));
        group.apply(|f| f.move_end());

        assert_eq!(group.pair_verdict(), Some(PairVerdict::Mismatch));
        assert_eq!(group.focused_index(), 0, "a mismatch resets the focus");
        assert_eq!(group.field(0).cursor(), 0);
        assert_eq!(group.field(1).cursor(), 0);
    }

    #[test]
    fn apply_action_reports_delete_char() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.apply(|f| f.insert('x'));
        group.apply(|f| f.move_home());

        assert_eq!(group.apply_action(Action::DeleteChar), FieldEdit::Deleted);
        assert_eq!(group.field(0).text(), "");
    }

    #[test]
    fn apply_action_moves_the_cursor_and_the_focus() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.apply(|f| f.insert('a'));
        group.apply(|f| f.insert('b'));

        assert_eq!(group.apply_action(Action::CursorHome), FieldEdit::Moved);
        assert_eq!(group.field(0).cursor(), 0);
        assert_eq!(group.apply_action(Action::CursorRight), FieldEdit::Moved);
        assert_eq!(group.field(0).cursor(), 1);
        assert_eq!(group.apply_action(Action::CursorEnd), FieldEdit::Moved);
        assert_eq!(group.field(0).cursor(), 2);

        assert_eq!(group.apply_action(Action::NextField), FieldEdit::Moved);
        assert_eq!(group.focused_index(), 1);
        assert_eq!(group.apply_action(Action::PrevField), FieldEdit::Moved);
        assert_eq!(group.focused_index(), 0);
    }

    #[test]
    fn edits_at_the_edges_are_no_ops() {
        let mut field = typed("ab");
        field.move_home();
        field.backspace();
        assert_eq!(field.text(), "ab", "backspace at the start changes nothing");

        field.move_end();
        field.delete();
        assert_eq!(field.text(), "ab", "delete at the end changes nothing");
    }

    #[test]
    fn an_empty_group_tolerates_every_operation() {
        let mut group = SecretFieldGroup::with_fields(0);

        group.next_field();
        group.prev_field();
        group.toggle_show();
        group.apply(|f| f.insert('x'));
        group.reset_cursors();
        group.clear();
        assert_eq!(group.focused_index(), 0);
        assert!(!group.show());
    }

    #[test]
    fn reset_cursors_keeps_the_text_but_clear_does_not() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.apply(|f| f.insert('x'));
        group.next_field();
        group.apply(|f| f.insert('y'));
        group.toggle_show();

        group.reset_cursors();
        assert_eq!(group.focused_index(), 0);
        assert_eq!(group.field(0).text(), "x");
        assert_eq!(group.field(1).text(), "y");
        assert_eq!(group.field(1).cursor(), 0);
        assert!(group.show(), "reset_cursors must keep the reveal flag");

        group.clear();
        assert_eq!(group.field(0).text(), "");
        assert_eq!(group.field(1).text(), "");
        assert!(!group.show(), "clear must hide the text again");
    }

    #[test]
    fn masked_value_hides_every_byte_for_multibyte_text() {
        let field = typed("й");
        let spans = secret_field_spans(&field, false, false);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "**");

        let revealed = secret_field_spans(&field, false, true);
        assert_eq!(revealed[0].content, "й");
    }

    #[test]
    fn focused_value_wraps_the_cursor_cell() {
        let mut field = typed("abc");
        field.move_left();
        field.move_left(); // cursor sits between `a` and `b`

        let spans = secret_field_spans(&field, true, true);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "abc");
        assert_eq!(spans.len(), 3, "before, cursor cell, after");
        assert_eq!(
            spans[1].content, "b",
            "the cell under the cursor is the next char"
        );
        assert_eq!(spans[1].style, ElementStyle::InvertedCursor.style());
    }

    #[test]
    fn password_field_spans_prefix_the_padded_label() {
        let field = typed("ab");
        let spans = password_field_spans("Pass", &field, false, false);

        assert_eq!(spans[0].content, "  Pass: ");
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "  Pass: **");
    }

    #[test]
    fn a_focused_empty_masked_field_renders_a_cursor_space() {
        let field = typed("");
        let spans = secret_field_spans(&field, true, false);

        assert_eq!(spans.len(), 1, "just the inverted cursor cell");
        assert_eq!(spans[0].content, " ");
        assert_eq!(spans[0].style, ElementStyle::InvertedCursor.style());
    }

    #[test]
    fn a_cursor_at_the_end_of_masked_text_wraps_the_trailing_space() {
        let field = typed("ab");
        let spans = secret_field_spans(&field, true, false);

        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "** ");
        assert_eq!(spans.len(), 2, "mask before, cursor space after");
        assert_eq!(spans[1].style, ElementStyle::InvertedCursor.style());
    }

    #[test]
    fn a_focused_masked_multibyte_field_keeps_the_cursor_on_a_star() {
        // `й` is two bytes, so the mask is two stars and the byte-offset cursor
        // maps onto a mask cell instead of a plaintext byte.
        let mut field = typed("й");
        field.move_home();

        let spans = secret_field_spans(&field, true, false);
        assert_eq!(
            spans[0].content, "*",
            "the cursor cell is the first mask star"
        );
        assert_eq!(spans[0].style, ElementStyle::InvertedCursor.style());

        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "**", "one star per byte");
    }
}
