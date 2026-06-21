use ratatui::style::{Color, Style};
use ratatui::text::Span;

/// Returns the `Style` used for a selected (highlighted) row.
pub fn selected_style(is_selected: bool) -> Style {
    if is_selected {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    }
}

/// Returns a `Span` for a selected/unselected row with `▸` prefix and highlight.
///
/// When `is_selected` is true the span has a `▸ ` prefix with DarkGray background;
/// otherwise a `  ` (two-space) prefix with default styling.
pub fn selected_span(text: &str, is_selected: bool) -> Span<'static> {
    let prefix = if is_selected { "▸ " } else { "  " };
    Span::styled(format!("{}{}", prefix, text), selected_style(is_selected))
}

/// Renders a text field with a cursor at `cursor` byte position.
///
/// When `is_active` is true, the character at the cursor is rendered as an
/// inverted block (White bg, Black fg). When inactive, the entire text is
/// dimmed (DarkGray fg).
pub fn cursor_field_spans(
    label: &str,
    text: &str,
    cursor: usize,
    is_active: bool,
) -> Vec<Span<'static>> {
    let label_style = Style::default().fg(Color::DarkGray);
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default().fg(Color::White);
    let inverted_style = Style::default().bg(Color::White).fg(Color::Black);

    if !is_active {
        return vec![
            Span::styled(label.to_string(), label_style),
            Span::styled(text.to_string(), dim_style),
        ];
    }

    let pos = cursor.min(text.len());
    let (before, rest) = text.split_at(pos);
    let ch = rest.chars().next();
    let after = &rest[ch.map(|c| c.len_utf8()).unwrap_or(0)..];

    let mut spans = vec![Span::styled(label.to_string(), label_style)];
    if !before.is_empty() {
        spans.push(Span::styled(before.to_string(), normal_style));
    }
    match ch {
        Some(c) => {
            let mut s = String::new();
            s.push(c);
            spans.push(Span::styled(s, inverted_style));
        }
        None => {
            spans.push(Span::styled(" ".to_string(), inverted_style));
        }
    }
    if !after.is_empty() {
        spans.push(Span::styled(after.to_string(), normal_style));
    }
    spans
}
