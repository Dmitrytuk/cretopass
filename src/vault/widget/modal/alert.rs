//! The passive alert card.
//!
//! An alert reports something the user must read — a saved configuration, a
//! failed validation, a refused action. It holds no input, so any key dismisses
//! it, exactly like the About card and as its own hint promises. The card itself
//! is drawn by [`crate::theming::render_alert`], shared with the Tree.

use std::borrow::Cow;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::HomeAction;
use crate::theming::{AlertKind, ElementStyle, render_alert, texts};

/// A one-line message shown over Home until the user acknowledges it.
pub struct Alert {
    /// The message the alert shows.
    pub message: String,
    /// Whether the message reports a success or a failure, which sets its colour.
    pub kind: AlertKind,
}

impl Alert {
    /// An alert reporting a positive outcome, drawn in the success colour.
    pub fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: AlertKind::Success,
        }
    }

    /// An alert reporting a refusal or a failure, drawn in the danger colour.
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: AlertKind::Error,
        }
    }

    /// Any key dismisses the alert: there is nothing to type into it.
    pub fn handle_key(&mut self, _key: KeyEvent) -> Option<HomeAction> {
        Some(HomeAction::Close)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        render_alert(frame, area, &self.message, self.kind);
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
        let mut alert = Alert::error("Saved.");
        assert!(matches!(
            alert.handle_key(KeyEvent::from(KeyCode::Char('x'))),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn the_alert_carries_no_mode_badge() {
        let alert = Alert::error("Saved.");
        let (mode, _, hint) = alert.status_parts();
        assert_eq!(mode, "");
        assert_eq!(hint.as_ref(), texts::HINT_ALERT);
    }

    #[test]
    fn the_constructor_records_the_outcome() {
        assert_eq!(Alert::success("ok").kind, AlertKind::Success);
        assert_eq!(Alert::error("no").kind, AlertKind::Error);
    }
}
