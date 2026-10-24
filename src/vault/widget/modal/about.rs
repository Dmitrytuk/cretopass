//! The About card: app identity, version, and the feedback links.
//!
//! It is a read-only info card, so any key dismisses it — the same rule as the
//! Tree's help overlay.

use std::borrow::Cow;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use super::HomeAction;
use crate::theming::{ElementStyle, texts};
use crate::vault::widget::chrome::paint_overlay;

/// The app identity card, opened with `?` from the Home menu.
pub struct About;

impl About {
    /// Any key dismisses the card: there is nothing to type into it.
    pub fn handle_key(&mut self, _key: KeyEvent) -> Option<HomeAction> {
        Some(HomeAction::Close)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let lines = lines();
        let overlay_height = (lines.len() as u16 + 2).min(area.height.saturating_sub(2));
        let width = area.width.saturating_sub(4);
        let overlay_area = Rect {
            x: area.width.saturating_sub(width) / 2,
            y: area.height.saturating_sub(overlay_height) / 2,
            width,
            height: overlay_height,
        };

        let block = Block::default()
            .title(texts::OVERLAY_ABOUT)
            .borders(Borders::ALL)
            .style(ElementStyle::HelpBlock.style());

        paint_overlay(
            frame,
            overlay_area,
            block,
            Paragraph::new(Text::from(lines))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false }),
        );
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        (
            texts::MODE_ABOUT,
            ElementStyle::ModeHelp,
            Cow::Borrowed(texts::HINT_HELP),
        )
    }
}

/// Builds the About body from the shared app-identity constants.
fn lines() -> Vec<Line<'static>> {
    let text = ElementStyle::HelpText.style();
    let styled = |body: String| Line::from(Span::styled(body, text));
    vec![
        styled(format!("{} {}", texts::APP_NAME, texts::APP_VERSION)),
        styled(texts::APP_TAGLINE.to_string()),
        Line::from(""),
        styled(texts::ABOUT_LABEL_BUGS.to_string()),
        styled(texts::ISSUES_URL.to_string()),
        Line::from(""),
        styled(texts::ABOUT_LABEL_SECURITY.to_string()),
        styled(texts::SECURITY_ADVISORY_URL.to_string()),
        Line::from(""),
        styled(format!(
            "{}{}",
            texts::ABOUT_LICENSE_PREFIX,
            texts::LICENSE_NAME
        )),
        styled(texts::LICENSE_URL.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn any_key_dismisses_the_card() {
        assert!(matches!(
            About.handle_key(KeyEvent::from(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn the_card_owns_the_about_badge_and_hint() {
        let (mode, _, hint) = About.status_parts();
        assert_eq!(mode, texts::MODE_ABOUT);
        assert_eq!(hint.as_ref(), texts::HINT_HELP);
    }
}
