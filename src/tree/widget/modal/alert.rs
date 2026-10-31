//! The passive alert modal.
//!
//! An alert reports something the user must read — an export result, a refused
//! action. It holds no input, so any key dismisses it. The card itself is drawn
//! by [`crate::theming::render_alert`], shared with Home.

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::TreeAction;
use crate::theming::{AlertKind, ElementStyle, render_alert, texts};
use crate::tree::widget::TreeState;

/// A one-line message shown over the Tree until the user acknowledges it.
#[derive(Debug)]
pub(crate) struct Alert {
    /// The message the alert shows.
    pub(crate) message: String,
    /// Whether the message reports a success or a failure, which sets its colour.
    pub(crate) kind: AlertKind,
}

impl Alert {
    /// An alert reporting a positive outcome, drawn in the success colour.
    pub(crate) fn success(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: AlertKind::Success,
        }
    }

    /// An alert reporting a refusal or a failure, drawn in the danger colour.
    pub(crate) fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: AlertKind::Error,
        }
    }

    /// Any key dismisses the alert: there is nothing to type into it.
    pub(crate) fn handle_key(
        &mut self,
        _key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        Some(TreeAction::Close)
    }

    /// Draws the message centred over `area`.
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        render_alert(frame, area, &self.message, self.kind);
    }

    /// Badge and hint for the tree status bar while the modal is open.
    pub(crate) fn status_parts(&self) -> (&'static str, ElementStyle, &'static str) {
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
        let mut alert = Alert::error("Exported.");
        let mut state = TreeState::new(crate::tree::node::Node::new("Root"));
        assert!(matches!(
            alert.handle_key(KeyEvent::from(KeyCode::Char('x')), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn the_alert_owns_its_status_parts() {
        let alert = Alert::error("Exported.");
        let (mode, _, hint) = alert.status_parts();
        assert_eq!(mode, texts::MODE_NORMAL);
        assert_eq!(hint, texts::HINT_DETAIL_ALERT);
    }

    #[test]
    fn the_constructor_records_the_outcome() {
        assert_eq!(Alert::success("ok").kind, AlertKind::Success);
        assert_eq!(Alert::error("no").kind, AlertKind::Error);
    }
}
