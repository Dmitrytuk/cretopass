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

pub fn selected_style(is_selected: bool) -> Style {
    if is_selected {
        ElementStyle::TreeRowSelected.style()
    } else {
        ElementStyle::TreeRow.style()
    }
}

pub fn selected_span(text: &str, is_selected: bool) -> Span<'static> {
    let prefix = if is_selected {
        Icon::SelectionArrow.as_str()
    } else {
        Icon::SelectionPad.as_str()
    };
    Span::styled(format!("{}{}", prefix, text), selected_style(is_selected))
}

pub fn cursor_field_spans(
    label: &str,
    text: &str,
    cursor: usize,
    is_active: bool,
) -> Vec<Span<'static>> {
    if !is_active {
        return vec![
            Span::styled(label.to_string(), ElementStyle::CursorLabel.style()),
            Span::styled(text.to_string(), ElementStyle::CursorDimmed.style()),
        ];
    }

    let pos = cursor.min(text.len());
    let (before, rest) = text.split_at(pos);
    let ch = rest.chars().next();
    let after = &rest[ch.map(|c| c.len_utf8()).unwrap_or(0)..];

    let mut spans = vec![Span::styled(
        label.to_string(),
        ElementStyle::CursorLabel.style(),
    )];
    if !before.is_empty() {
        spans.push(Span::styled(
            before.to_string(),
            ElementStyle::CursorNormalText.style(),
        ));
    }
    match ch {
        Some(c) => {
            let mut s = String::new();
            s.push(c);
            spans.push(Span::styled(s, ElementStyle::InvertedCursor.style()));
        }
        None => {
            spans.push(Span::styled(
                " ".to_string(),
                ElementStyle::InvertedCursor.style(),
            ));
        }
    }
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
}
