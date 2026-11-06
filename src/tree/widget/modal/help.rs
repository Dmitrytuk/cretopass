//! The Help modal: a read-only list of the Tree's key bindings.
//!
//! Opened with `?`. There is nothing to type into it, so any key dismisses it,
//! exactly as its hint promises.

use crossterm::event::{KeyEvent, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use crate::hitbox::is_left;
use crate::theming::{ElementStyle, texts};
use crate::tree::widget::TreeState;

/// The Tree's key-binding list, opened with `?`.
#[derive(Debug)]
pub(crate) struct Help;

impl Help {
    /// Any key dismisses the modal: there is nothing to type into it.
    pub(crate) fn handle_key(
        &mut self,
        _key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        Some(TreeAction::Close)
    }

    /// A left click anywhere dismisses the modal; the right button is ignored.
    pub(crate) fn handle_click(
        &mut self,
        button: MouseButton,
        _x: u16,
        _y: u16,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        is_left(button).then_some(TreeAction::Close)
    }

    /// Draws the key-binding list centred over `area`.
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let n_bindings = texts::TREE_HELP_KEY_BINDINGS.len() as u16;
        let overlay_height = (n_bindings + 3).min(area.height.saturating_sub(2));
        let overlay_area = Rect {
            x: area.width / 6,
            y: area.height.saturating_sub(overlay_height) / 2,
            width: area.width * 2 / 3,
            height: overlay_height,
        };

        let block = Block::default()
            .title(texts::OVERLAY_HELP)
            .borders(Borders::ALL)
            .style(ElementStyle::HelpBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = Vec::new();
        for binding in texts::TREE_HELP_KEY_BINDINGS {
            lines.push(Line::from(Span::styled(
                *binding,
                ElementStyle::HelpText.style(),
            )));
        }
        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Badge and hint for the tree status bar while the modal is open.
    pub(crate) fn status_parts(&self) -> (&'static str, ElementStyle, &'static str) {
        (texts::MODE_HELP, ElementStyle::ModeHelp, texts::HINT_HELP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    #[test]
    fn any_key_dismisses_the_list() {
        let mut help = Help;
        let mut state = TreeState::new(crate::tree::node::Node::new("Root"));
        assert!(matches!(
            help.handle_key(KeyEvent::from(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
        assert!(matches!(
            help.handle_key(KeyEvent::from(KeyCode::Char('x')), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn the_list_owns_the_help_badge_and_hint() {
        let (mode, _, hint) = Help.status_parts();
        assert_eq!(mode, texts::MODE_HELP);
        assert_eq!(hint, texts::HINT_HELP);
    }

    #[test]
    fn the_binding_list_documents_the_root_exception() {
        let root_line = texts::TREE_HELP_KEY_BINDINGS
            .iter()
            .find(|binding| binding.trim_start().starts_with("Root"))
            .expect("the Root's restricted action set must be documented");
        assert!(
            root_line.contains("cut") && root_line.contains("sibling"),
            "the Root line must name the withheld actions: {root_line:?}"
        );
    }
}
