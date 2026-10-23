//! The passive alert card.
//!
//! An alert reports something the user must read — a saved configuration, a
//! failed validation, a refused action. It holds no input, so any key dismisses
//! it, exactly like the About card and as its own hint promises.

use std::borrow::Cow;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theming::{ElementStyle, texts};
use crate::vault::widget::HomeAction;
use crate::vault::widget::chrome::{make_overlay, paint_overlay};

/// A one-line message shown over Home until the user acknowledges it.
pub struct Alert {
    /// The message the alert shows.
    pub message: String,
}

impl Alert {
    /// Any key dismisses the alert: there is nothing to type into it.
    pub fn handle_key(&mut self, _key: KeyEvent) -> Option<HomeAction> {
        Some(HomeAction::Close)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = make_overlay(area, 5);
        let block = Block::default()
            .title(texts::ALERT_TITLE)
            .borders(Borders::ALL)
            .style(ElementStyle::HomeAlertBlock.style());

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                self.message.clone(),
                ElementStyle::ConfirmMessage.style(),
            )),
            Line::from(""),
        ];

        paint_overlay(
            frame,
            overlay_area,
            block,
            Paragraph::new(Text::from(lines)).alignment(Alignment::Center),
        );
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        (
            "",
            ElementStyle::ModeNormal,
            Cow::Borrowed(texts::HINT_ALERT),
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
            message: "Saved.".to_string(),
        };
        assert!(matches!(
            alert.handle_key(KeyEvent::from(KeyCode::Char('x'))),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn the_alert_carries_no_mode_badge() {
        let alert = Alert {
            message: "Saved.".to_string(),
        };
        let (mode, _, hint) = alert.status_parts();
        assert_eq!(mode, "");
        assert_eq!(hint.as_ref(), texts::HINT_ALERT);
    }
}
