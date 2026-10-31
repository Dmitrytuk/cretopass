//! The shared alert card.
//!
//! An alert reports something the user must read — a successful export, an
//! import, a refused action. It holds no input. The Home and Tree screens keep
//! their own modal wrappers for key handling and status-bar parts, but both draw
//! this one card through [`render_alert`], so an outcome never looks different
//! from one screen to the other.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::style::ElementStyle;
use super::texts;

/// What an alert reports, which decides how it is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    /// A positive outcome: an export, an import, a saved setting, a copy.
    Success,
    /// A refusal or a failure the user must act on.
    Error,
}

impl AlertKind {
    /// The border and title style for this outcome.
    fn block(self) -> ElementStyle {
        match self {
            Self::Success => ElementStyle::AlertSuccessBlock,
            Self::Error => ElementStyle::AlertErrorBlock,
        }
    }

    /// The message style for this outcome.
    fn text(self) -> ElementStyle {
        match self {
            Self::Success => ElementStyle::AlertSuccessText,
            Self::Error => ElementStyle::AlertErrorText,
        }
    }
}

/// Draws the alert card centred over `area`.
///
/// Five rows tall and three-fifths of the screen wide, clamped so it never
/// covers the last row; the message is centred in the card.
pub fn render_alert(frame: &mut Frame, area: Rect, message: &str, kind: AlertKind) {
    let overlay_area = centered_overlay(area);
    let block = Block::default()
        .title(texts::ALERT_TITLE)
        .borders(Borders::ALL)
        .style(kind.block().style());

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, kind.text().style())),
        Line::from(""),
    ];

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).alignment(Alignment::Center),
        inner,
    );
}

/// A centred overlay five rows tall, three-fifths of the screen wide.
fn centered_overlay(area: Rect) -> Rect {
    let height = 5.min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 5,
        y: area.height.saturating_sub(height) / 2,
        width: area.width.saturating_mul(3) / 5,
        height,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::theming::palette;

    /// Renders an alert and returns the whole buffer as a string.
    fn render_to_text(kind: AlertKind) -> String {
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_alert(frame, area, "saved", kind);
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn both_kinds_draw_the_message_and_title() {
        for kind in [AlertKind::Success, AlertKind::Error] {
            let text = render_to_text(kind);
            assert!(
                text.contains("saved"),
                "message missing for {kind:?}: {text}"
            );
            assert!(text.contains(texts::ALERT_TITLE.trim()));
        }
    }

    #[test]
    fn the_block_border_is_coloured_by_outcome() {
        let scheme = palette();
        assert_eq!(AlertKind::Success.block().style().fg, Some(scheme.success));
        assert_eq!(AlertKind::Error.block().style().fg, Some(scheme.danger));
    }

    #[test]
    fn the_message_is_coloured_by_outcome() {
        let scheme = palette();
        assert_eq!(AlertKind::Success.text().style().fg, Some(scheme.success));
        assert_eq!(AlertKind::Error.text().style().fg, Some(scheme.text));
    }

    #[test]
    fn the_overlay_stays_inside_a_terminal_smaller_than_it() {
        let rect = centered_overlay(Rect::new(0, 0, 2, 2));
        assert!(rect.right() <= 2);
        assert!(rect.bottom() <= 2);
    }
}
