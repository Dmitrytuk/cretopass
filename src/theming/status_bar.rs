//! Unified status-bar renderer shared by every screen.
//!
//! Each screen draws exactly one status line: a mode badge followed by context
//! hints. The badge colour encodes the risk/importance of the active action
//! (see ADR 0014) and means the same thing in every screen; the hints are
//! passive text reused from [`crate::theming::texts`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::theming::ElementStyle;

/// Draws the unified status bar as `[MODE] hints`.
///
/// `mode_str` is the bare label (no padding); the function surrounds it with
/// spaces and renders it as a filled badge. `mode_style` is the badge style for
/// the active importance category. `hint` is the passive keybinding hint shown
/// after the badge. An empty `mode_str` omits the badge and draws the hint
/// alone.
pub fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    mode_str: &str,
    mode_style: Style,
    hint: &str,
) {
    let mut spans: Vec<Span> = Vec::with_capacity(3);
    if !mode_str.is_empty() {
        spans.push(Span::styled(
            format!(" {mode_str} "),
            mode_style.add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(hint, ElementStyle::StatusBarText.style()));

    frame.render_widget(Paragraph::new(Text::from(Line::from(spans))), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    fn render_to_string(mode_str: &str, hint: &str) -> String {
        let mut terminal = Terminal::new(TestBackend::new(40, 1)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_status_bar(
                    frame,
                    area,
                    mode_str,
                    ElementStyle::ModeNormal.style(),
                    hint,
                );
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn draws_the_badge_then_the_hint() {
        let text = render_to_string("NORMAL", "j/k:nav");
        assert!(text.starts_with(" NORMAL "), "badge should lead: {text:?}");
        assert!(text.contains("j/k:nav"), "hint should follow: {text:?}");
    }

    #[test]
    fn empty_mode_omits_the_badge() {
        let text = render_to_string("", "Press any key");
        assert!(text.starts_with("Press any key"), "got: {text:?}");
    }

    #[test]
    fn badge_uses_the_supplied_style() {
        let mut terminal = Terminal::new(TestBackend::new(20, 1)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_status_bar(
                    frame,
                    area,
                    "CUT",
                    ElementStyle::ModePendingCut.style(),
                    "esc:cancel",
                );
            })
            .unwrap();

        // Cell 0 is the padding space, cell 1 the first glyph of the badge.
        let cell = terminal.backend().buffer().content[1].clone();
        assert_eq!(cell.symbol(), "C");
        assert_eq!(cell.style().bg, Some(Color::Red));
        assert!(cell.style().add_modifier.contains(Modifier::BOLD));
    }
}
