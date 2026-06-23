use ratatui::style::Style;
use ratatui::text::Span;

use crate::theming::{ElementStyle, Icon};

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

    let mut spans = vec![Span::styled(label.to_string(), ElementStyle::CursorLabel.style())];
    if !before.is_empty() {
        spans.push(Span::styled(before.to_string(), ElementStyle::CursorNormalText.style()));
    }
    match ch {
        Some(c) => {
            let mut s = String::new();
            s.push(c);
            spans.push(Span::styled(s, ElementStyle::InvertedCursor.style()));
        }
        None => {
            spans.push(Span::styled(" ".to_string(), ElementStyle::InvertedCursor.style()));
        }
    }
    if !after.is_empty() {
        spans.push(Span::styled(after.to_string(), ElementStyle::CursorNormalText.style()));
    }
    spans
}
