//! The passive alert modal.
//!
//! An alert reports something the user must read — an export result, a refused
//! action. It holds no input, so any key dismisses it.

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use super::chrome::centered_rect;
use crate::theming::{ElementStyle, texts};
use crate::tree::widget::TreeState;

/// A one-line message shown over the Tree until the user acknowledges it.
#[derive(Debug)]
pub struct Alert {
    /// The message the alert shows.
    pub message: String,
}

impl Alert {
    /// Any key dismisses the alert: there is nothing to type into it.
    pub fn handle_key(&mut self, _key: KeyEvent, _state: &mut TreeState) -> Option<TreeAction> {
        Some(TreeAction::Close)
    }

    /// Draws the message centred over `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = centered_rect(area, area.width * 2 / 3, 4);

        let block = Block::default()
            .title(texts::ALERT_TITLE)
            .borders(Borders::ALL)
            .style(ElementStyle::CopiedBlock.style());

        let lines = vec![
            Line::from(Span::styled(
                format!(" {}", self.message),
                ElementStyle::CopiedText.style(),
            )),
            Line::from(""),
        ];

        let inner = block.inner(overlay_area);
        frame.render_widget(Clear, overlay_area);
        frame.render_widget(&block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Badge and hint for the tree status bar while the modal is open.
    pub fn status_parts(&self) -> (&'static str, ElementStyle, &'static str) {
        (
            texts::MODE_NORMAL,
            ElementStyle::ModeNormal,
            texts::HINT_DETAIL_ALERT,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn any_key_dismisses_the_alert() {
        let mut alert = Alert {
            message: "Exported.".to_string(),
        };
        let mut state = TreeState::new(crate::tree::node::Node::new("Root"));
        assert!(matches!(
            alert.handle_key(KeyEvent::from(KeyCode::Char('x')), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn the_alert_owns_its_status_parts() {
        let alert = Alert {
            message: "Exported.".to_string(),
        };
        let (mode, _, hint) = alert.status_parts();
        assert_eq!(mode, texts::MODE_NORMAL);
        assert_eq!(hint, texts::HINT_DETAIL_ALERT);
    }
}
