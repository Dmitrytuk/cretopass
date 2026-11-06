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

/// Inserts a character at the cursor, advancing the cursor past the inserted
/// character (accounting for multi-byte UTF-8 chars).
pub fn insert_char_at(text: &mut String, cursor: &mut usize, c: char) {
    let pos = (*cursor).min(text.len());
    text.insert(pos, c);
    *cursor = pos + c.len_utf8();
}

/// Inserts the whole string `s` at the cursor, advancing the cursor past it.
///
/// The cursor is clamped to the text length and, defensively, walked back to a
/// char boundary first, so a stray offset can never panic the insertion. An
/// empty `s` changes nothing.
pub fn insert_str_at(text: &mut String, cursor: &mut usize, s: &str) {
    let mut pos = (*cursor).min(text.len());
    if !text.is_char_boundary(pos) {
        pos = cursor_left(text, pos);
    }
    text.insert_str(pos, s);
    *cursor = pos + s.len();
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
    pub fn clean(self, text: &str) -> Cow<'_, str> {
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
}
