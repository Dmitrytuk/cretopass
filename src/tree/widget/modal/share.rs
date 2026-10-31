//! The Share modal: a one-step wizard that picks what to share.
//!
//! `This branch` shares the selected Node and its descendants; `Whole file`
//! shares the open file's Root. Confirming reports a
//! [`TreeAction::BeginShareDestination`] — naming the destination folder and
//! writing the bundle are the orchestrator's job.

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use super::chrome::centered_rect;
use crate::cursor::selected_span;
use crate::share::ShareScope;
use crate::theming::{Action, ElementStyle, texts};
use crate::tree::node::Node;
use crate::tree::widget::TreeState;

/// The Share modal: pick the scope.
#[derive(Debug)]
pub(crate) struct ShareWizard {
    /// The Node selected when the wizard opened; the branch scope shares it.
    node: Node,
    /// Focused row: 0 = this branch, 1 = whole file.
    selected: usize,
}

impl ShareWizard {
    /// A wizard for sharing `node` (or, on the whole-file row, the open Root),
    /// starting on the branch row.
    pub(crate) fn new(node: Node) -> Self {
        Self { node, selected: 0 }
    }

    /// Reads a key: [`TreeAction::Close`] on `Esc`,
    /// [`TreeAction::BeginShareDestination`] once the scope is confirmed.
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        match Action::from_select_field_type(&key) {
            Action::Cancel => Some(TreeAction::Close),
            Action::NavDown | Action::NavUp => {
                self.selected = (self.selected + 1) % 2;
                None
            }
            Action::SelectIndex(i) if i < 2 => {
                self.selected = i;
                None
            }
            Action::Confirm => Some(TreeAction::BeginShareDestination {
                node: self.node.clone(),
                scope: self.scope(),
            }),
            _ => None,
        }
    }

    /// The scope the current selection stands for.
    fn scope(&self) -> ShareScope {
        if self.selected == 0 {
            ShareScope::Branch
        } else {
            ShareScope::WholeFile
        }
    }

    /// Draws the scope list over `area`.
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = centered_rect(area, area.width / 2, 6);

        let block = Block::default()
            .title(texts::OVERLAY_SHARE_SCOPE)
            .borders(Borders::ALL)
            .style(ElementStyle::FieldTypeBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = Vec::new();

        for (i, item) in texts::SHARE_SCOPE_OPTIONS.iter().enumerate() {
            lines.push(Line::from(selected_span(item, i == self.selected)));
        }

        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Badge and hint for the tree status bar while the wizard is open.
    pub(crate) fn status_parts(&self) -> (&'static str, ElementStyle, &'static str) {
        (
            texts::MODE_SHARE,
            ElementStyle::ModeFieldType,
            texts::HINT_SHARE_SCOPE,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::Node;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn wizard() -> (ShareWizard, TreeState) {
        (
            ShareWizard::new(Node::new("Work")),
            TreeState::new(Node::new("Root")),
        )
    }

    #[test]
    fn the_branch_row_is_chosen_by_default() {
        let (mut wizard, mut state) = wizard();

        match wizard.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::BeginShareDestination { node, scope }) => {
                assert_eq!(node.name, "Work");
                assert_eq!(scope, ShareScope::Branch);
            }
            _ => panic!("expected a share destination request"),
        }
    }

    #[test]
    fn the_whole_file_row_is_reachable_by_j() {
        let (mut wizard, mut state) = wizard();
        let _ = wizard.handle_key(key(KeyCode::Char('j')), &mut state);
        match wizard.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::BeginShareDestination { scope, .. }) => {
                assert_eq!(scope, ShareScope::WholeFile);
            }
            _ => panic!("expected a share destination request"),
        }
    }

    #[test]
    fn the_scope_can_be_chosen_by_number() {
        let (mut wizard, mut state) = wizard();
        let _ = wizard.handle_key(key(KeyCode::Char('2')), &mut state);
        match wizard.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::BeginShareDestination { scope, .. }) => {
                assert_eq!(scope, ShareScope::WholeFile);
            }
            _ => panic!("expected a share destination request"),
        }
    }

    #[test]
    fn esc_closes_the_wizard() {
        let (mut wizard, mut state) = wizard();
        assert!(matches!(
            wizard.handle_key(key(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn status_parts_advertise_the_scope_step() {
        let (wizard, _state) = wizard();
        let (mode, _, hint) = wizard.status_parts();
        assert_eq!(mode, texts::MODE_SHARE);
        assert_eq!(hint, texts::HINT_SHARE_SCOPE);
    }
}
