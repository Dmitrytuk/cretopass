use std::borrow::Cow;

use ratatui::style::Style;
use ratatui::text::Span;

use crate::theming::{ElementStyle, Icon};

/// Moves a byte-offset cursor one character to the left, always landing on a
/// UTF-8 char boundary.
pub fn cursor_left(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if cursor == 0 {
        return 0;
    }
    let mut idx = cursor - 1;
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

/// Moves a byte-offset cursor one character to the right, always landing on a
/// UTF-8 char boundary.
pub fn cursor_right(text: &str, cursor: usize) -> usize {
    let cursor = cursor.min(text.len());
    if cursor >= text.len() {
        return text.len();
    }
    text[cursor..]
        .chars()
        .next()
        .map_or(cursor, |c| cursor + c.len_utf8())
}

/// Removes the character immediately before the cursor, returning the new
/// cursor position (the byte offset of the removed character's start).
pub fn backspace_at(text: &mut String, cursor: usize) -> usize {
    let new_cursor = cursor_left(text, cursor);
    if new_cursor < cursor.min(text.len()) {
        text.remove(new_cursor);
    }
    new_cursor
}

/// Removes the character at the cursor position (forward delete).
pub fn delete_at(text: &mut String, cursor: usize) {
    let cursor = cursor.min(text.len());
    if cursor < text.len() {
        text.remove(cursor);
    }
}

/// The most bytes a single text field may hold.
///
/// A bracketed paste hands over whatever the terminal buffered and the
/// clipboard helpers are uncapped, so an untrusted payload can be arbitrarily
/// large before the app ever sees it. This bound keeps one field from growing
/// without limit: [`PastePolicy::clean`] truncates an accepted paste to it and
/// every insertion ([`insert_char_at`], [`insert_str_at`], [`push_str_capped`],
/// [`push_char_capped`]) stops at it. It is a memory guard, not field
/// validation: text loaded from a vault may already be longer, and stays
/// untouched.
pub(crate) const MAX_FIELD_BYTES: usize = 8 * 1024;

/// Cuts `text` down to at most `max_bytes` bytes without splitting a UTF-8
/// character. Borrows the whole slice when it already fits.
fn truncate_to_bytes(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// Inserts a character at the cursor, advancing the cursor past the inserted
/// character (accounting for multi-byte UTF-8 chars).
///
/// A character that would push the field past [`MAX_FIELD_BYTES`] is not
/// inserted; the cursor is clamped to the text length either way.
pub fn insert_char_at(text: &mut String, cursor: &mut usize, c: char) {
    let pos = (*cursor).min(text.len());
    if text.len() + c.len_utf8() > MAX_FIELD_BYTES {
        *cursor = pos;
        return;
    }
    text.insert(pos, c);
    *cursor = pos + c.len_utf8();
}

/// Inserts the whole string `s` at the cursor, advancing the cursor past it.
///
/// The cursor is clamped to the text length and, defensively, walked back to a
/// char boundary first, so a stray offset can never panic the insertion. An
/// empty `s`, or one that has no room left before [`MAX_FIELD_BYTES`], changes
/// nothing.
pub fn insert_str_at(text: &mut String, cursor: &mut usize, s: &str) {
    let mut pos = (*cursor).min(text.len());
    if !text.is_char_boundary(pos) {
        pos = cursor_left(text, pos);
    }
    let s = truncate_to_bytes(s, MAX_FIELD_BYTES.saturating_sub(text.len()));
    if s.is_empty() {
        *cursor = pos;
        return;
    }
    text.insert_str(pos, s);
    *cursor = pos + s.len();
}

/// Appends `s` to `target`, stopping at [`MAX_FIELD_BYTES`].
///
/// The append counterpart of [`insert_str_at`] for the plain `String` fields
/// that keep no cursor (the search boxes and the custom field-name prompt).
pub(crate) fn push_str_capped(target: &mut String, s: &str) {
    let room = MAX_FIELD_BYTES.saturating_sub(target.len());
    target.push_str(truncate_to_bytes(s, room));
}

/// Appends `c` to `target`, stopping at [`MAX_FIELD_BYTES`].
///
/// The single-character counterpart of [`push_str_capped`]; used by the typed
/// input arms of the cursor-less fields.
pub(crate) fn push_char_capped(target: &mut String, c: char) {
    if target.len() + c.len_utf8() <= MAX_FIELD_BYTES {
        target.push(c);
    }
}

/// Removes every control character from `text`, borrowing when there are none.
///
/// Used for single-line fields, where a line break is as unwanted as an escape
/// sequence. (`ratatui`'s `Span` renderer filters `\n` but no other control
/// character; stripping all of them keeps a single-line field strictly
/// single-line.)
pub(crate) fn strip_control_chars(text: &str) -> Cow<'_, str> {
    if text.chars().any(char::is_control) {
        Cow::Owned(text.chars().filter(|c| !c.is_control()).collect())
    } else {
        Cow::Borrowed(text)
    }
}

/// How a pasted string is sanitised before it enters a single-line field.
///
/// A terminal clipboard often carries trailing newlines or entirely unrelated
/// text; the policy is the one place that decides what a field will accept, so
/// every paste surface funnels through the same rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PastePolicy {
    /// A single-line field: control characters (line breaks, `ESC`, `BEL`, …)
    /// are dropped, everything else kept.
    SingleLine,
    /// A digits-only field (the generator length): everything but ASCII digits
    /// is dropped.
    Digits,
}

impl PastePolicy {
    /// Returns the form of `text` this policy accepts, borrowing when nothing
    /// needs to be removed.
    ///
    /// The payload is first cut to [`MAX_FIELD_BYTES`], so a hostile clipboard
    /// cannot retain an unbounded string in a field even before sanitising.
    pub fn clean(self, text: &str) -> Cow<'_, str> {
        let text = truncate_to_bytes(text, MAX_FIELD_BYTES);
        match self {
            Self::SingleLine => strip_control_chars(text),
            Self::Digits if text.chars().all(|c| c.is_ascii_digit()) => Cow::Borrowed(text),
            Self::Digits => Cow::Owned(text.chars().filter(char::is_ascii_digit).collect()),
        }
    }
}

/// The style for a list row: the selection background when `is_selected`, the
/// plain tree-row style otherwise.
pub fn selected_style(is_selected: bool) -> Style {
    if is_selected {
        ElementStyle::ListRowSelected.style()
    } else {
        ElementStyle::TreeRow.style()
    }
}

/// A list-row span: `text` prefixed by the selection arrow (or its pad) and
/// styled by [`selected_style`].
pub fn selected_span(text: &str, is_selected: bool) -> Span<'static> {
    let prefix = if is_selected {
        Icon::SelectionArrow.as_str()
    } else {
        Icon::SelectionPad.as_str()
    };
    Span::styled(format!("{}{}", prefix, text), selected_style(is_selected))
}

/// Splits `text` at the byte offset `cursor` into the slice before the cursor,
/// the character under it (if any), and the slice after it. The offset is
/// clamped to the text length, so a cursor past the end yields an empty `after`
/// and no character under the cursor.
///
/// The shared cursor-cell decomposition behind [`cursor_field_spans`] and
/// [`crate::cursor::secret_field_spans`]; each caller applies its own styles.
pub(crate) fn split_at_cursor(text: &str, cursor: usize) -> (&str, Option<char>, &str) {
    let pos = cursor.min(text.len());
    let (before, rest) = text.split_at(pos);
    let ch = rest.chars().next();
    let after = &rest[ch.map_or(0, char::len_utf8)..];
    (before, ch, after)
}

/// Spans for `text` prefixed by `label`, with the character under `cursor`
/// rendered as an inverted cursor cell.
pub fn cursor_field_spans(label: &str, text: &str, cursor: usize) -> Vec<Span<'static>> {
    let (before, ch, after) = split_at_cursor(text, cursor);

    let mut spans = Vec::with_capacity(4);
    spans.push(Span::styled(
        label.to_string(),
        ElementStyle::CursorLabel.style(),
    ));
    if !before.is_empty() {
        spans.push(Span::styled(
            before.to_string(),
            ElementStyle::CursorNormalText.style(),
        ));
    }
    spans.push(Span::styled(
        ch.map_or_else(|| " ".to_string(), |c| c.to_string()),
        ElementStyle::InvertedCursor.style(),
    ));
    if !after.is_empty() {
        spans.push(Span::styled(
            after.to_string(),
            ElementStyle::CursorNormalText.style(),
        ));
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_left_moves_over_multibyte_char() {
        let text = "aé";
        assert_eq!(cursor_left(text, text.len()), 1);
        assert_eq!(cursor_left(text, 1), 0);
        assert_eq!(cursor_left(text, 0), 0);
    }

    #[test]
    fn cursor_right_moves_over_multibyte_char() {
        let text = "éa";
        assert_eq!(cursor_right(text, 0), 2);
        assert_eq!(cursor_right(text, 2), text.len());
        assert_eq!(cursor_right(text, text.len()), text.len());
    }

    #[test]
    fn backspace_removes_multibyte_char() {
        let mut text = "aé".to_string();
        let cursor = backspace_at(&mut text, 3);
        assert_eq!(text, "a");
        assert_eq!(cursor, 1);

        let cursor = backspace_at(&mut text, 1);
        assert_eq!(text, "");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn delete_removes_multibyte_char() {
        let mut text = "éa".to_string();
        delete_at(&mut text, 0);
        assert_eq!(text, "a");

        let mut text = "aé".to_string();
        delete_at(&mut text, 1);
        assert_eq!(text, "a");
    }

    #[test]
    fn insert_char_advances_past_multibyte() {
        let mut text = "a".to_string();
        let mut cursor = 1;
        insert_char_at(&mut text, &mut cursor, 'é');
        assert_eq!(text, "aé");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn insert_str_inserts_whole_string_and_advances() {
        let mut text = "aç".to_string();
        let mut cursor = 1; // between `a` and `ç`
        insert_str_at(&mut text, &mut cursor, "XY");
        assert_eq!(text, "aXYç");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn insert_str_appends_when_the_cursor_is_past_the_end() {
        let mut text = "ab".to_string();
        let mut cursor = 99;
        insert_str_at(&mut text, &mut cursor, "c");
        assert_eq!(text, "abc");
        assert_eq!(cursor, 3);
    }

    #[test]
    fn insert_str_on_a_stray_mid_char_cursor_lands_on_a_boundary() {
        let mut text = "é".to_string(); // 2 bytes
        let mut cursor = 1; // inside `é`
        insert_str_at(&mut text, &mut cursor, "a");
        assert_eq!(text, "aé");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn single_line_policy_drops_line_breaks_and_borrows_when_clean() {
        assert_eq!(PastePolicy::SingleLine.clean("a\r\nb\nc"), "abc");
        assert!(matches!(
            PastePolicy::SingleLine.clean("plain"),
            Cow::Borrowed("plain")
        ));
    }

    #[test]
    fn digits_policy_keeps_only_ascii_digits_and_borrows_when_clean() {
        assert_eq!(PastePolicy::Digits.clean("1a2b3"), "123");
        assert_eq!(PastePolicy::Digits.clean("no digits"), "");
        assert!(matches!(
            PastePolicy::Digits.clean("42"),
            Cow::Borrowed("42")
        ));
    }

    #[test]
    fn strip_control_chars_borrows_clean_text_and_removes_controls() {
        assert!(matches!(
            strip_control_chars("plain text"),
            Cow::Borrowed("plain text")
        ));
        assert_eq!(strip_control_chars("a\u{1b}[2Jb"), "a[2Jb");
        assert_eq!(strip_control_chars("x\u{7}y\u{0}z"), "xyz");
        assert_eq!(strip_control_chars("\u{1b}\u{7}"), "");
    }

    #[test]
    fn single_line_policy_drops_terminal_escape_sequences() {
        // A clipboard can carry a raw CSI sequence; the field must never keep
        // the `ESC` that would let it drive the terminal at render time.
        assert_eq!(PastePolicy::SingleLine.clean("pw\u{1b}[201~"), "pw[201~");
        assert_eq!(PastePolicy::SingleLine.clean("a\tb"), "ab");
    }

    #[test]
    fn paste_policy_truncates_an_oversized_payload() {
        let huge = "a".repeat(MAX_FIELD_BYTES + 100);
        assert_eq!(
            PastePolicy::SingleLine.clean(&huge).len(),
            MAX_FIELD_BYTES,
            "a single paste stops at the field bound"
        );

        let huge_digits = "7".repeat(MAX_FIELD_BYTES + 100);
        assert_eq!(
            PastePolicy::Digits.clean(&huge_digits).len(),
            MAX_FIELD_BYTES
        );
    }

    #[test]
    fn paste_policy_truncation_never_splits_a_character() {
        // Four-byte chars: the cap (~8 KiB) is not a multiple of four, so a
        // naive byte cut would land inside a char.
        let huge = "\u{1f600}".repeat(MAX_FIELD_BYTES);
        let cleaned = PastePolicy::SingleLine.clean(&huge);
        assert!(cleaned.len() <= MAX_FIELD_BYTES);
        assert_eq!(cleaned.chars().count(), cleaned.len() / 4);
    }

    #[test]
    fn insert_char_at_stops_at_the_field_bound() {
        let mut text = "a".repeat(MAX_FIELD_BYTES - 1);
        let mut cursor = text.len();
        insert_char_at(&mut text, &mut cursor, 'b');
        insert_char_at(&mut text, &mut cursor, 'c');
        assert_eq!(text.len(), MAX_FIELD_BYTES);
        assert!(text.ends_with('b'), "the overflowing char is dropped");
        assert_eq!(cursor, MAX_FIELD_BYTES);
    }

    #[test]
    fn insert_str_at_fills_only_the_remaining_room() {
        let mut text = "a".repeat(MAX_FIELD_BYTES - 2);
        let mut cursor = text.len();
        insert_str_at(&mut text, &mut cursor, "XYZ");
        assert_eq!(text.len(), MAX_FIELD_BYTES);
        assert!(text.ends_with("XY"));
        assert_eq!(cursor, MAX_FIELD_BYTES);

        // A full field accepts nothing and leaves the cursor untouched.
        insert_str_at(&mut text, &mut cursor, "Z");
        assert_eq!(text.len(), MAX_FIELD_BYTES);
        assert_eq!(cursor, MAX_FIELD_BYTES);
    }

    #[test]
    fn push_str_capped_appends_within_the_bound() {
        let mut target = "a".repeat(MAX_FIELD_BYTES - 3);
        push_str_capped(&mut target, "XYZ");
        assert_eq!(target.len(), MAX_FIELD_BYTES);
        assert!(target.ends_with("XYZ"));

        push_str_capped(&mut target, "more");
        assert_eq!(target.len(), MAX_FIELD_BYTES);
    }

    #[test]
    fn push_char_capped_stops_at_the_bound() {
        let mut target = "a".repeat(MAX_FIELD_BYTES - 1);
        push_char_capped(&mut target, 'é'); // 2 bytes, but only 1 byte of room
        assert_eq!(target.len(), MAX_FIELD_BYTES - 1);
        push_char_capped(&mut target, 'b');
        assert_eq!(target.len(), MAX_FIELD_BYTES);
    }

    #[test]
    fn insert_str_at_skips_a_multibyte_char_with_no_room() {
        let mut text = "a".repeat(MAX_FIELD_BYTES - 1);
        let mut cursor = text.len();
        insert_str_at(&mut text, &mut cursor, "é"); // 2 bytes, 1 byte of room
        assert_eq!(text.len(), MAX_FIELD_BYTES - 1);
        assert_eq!(cursor, MAX_FIELD_BYTES - 1);
    }

    #[test]
    fn an_over_bound_value_is_preserved_while_new_insertion_stops() {
        // Loaded vault text may already be longer than the bound; the guard
        // must not truncate it, only refuse to grow it.
        let mut text = "a".repeat(MAX_FIELD_BYTES + 10);
        let mut cursor = text.len();
        insert_char_at(&mut text, &mut cursor, 'b');
        insert_str_at(&mut text, &mut cursor, "XYZ");
        assert_eq!(text.len(), MAX_FIELD_BYTES + 10);
    }
}
