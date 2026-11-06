//! The Share modal: a one-step wizard that picks what to share.
//!
//! `This node` shares the selected Node alone; `This node with children` shares
//! it and its descendants — the same choice the export wizard offers. Confirming
//! reports a [`TreeAction::BeginShareDestination`] — naming the destination
//! folder and writing the bundle are the orchestrator's job.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use super::chrome::centered_rect;
use crate::cursor::selected_span;
use crate::hitbox::{ClickSurface, RowClick, record_rows};
use crate::theming::{Action, ElementStyle, texts};
use crate::tree::node::Node;
use crate::tree::widget::TreeState;

/// The Share modal: pick what to share.
#[derive(Debug)]
pub(crate) struct ShareWizard {
    /// The Node selected when the wizard opened.
    node: Node,
    /// Focused row: 0 = this node, 1 = this node with children.
    selected: usize,
    /// The clickable row regions and the panel the renderer drew, so a left click
    /// selects a scope and a click outside the panel closes the wizard.
    clicks: ClickSurface<usize>,
}

impl ShareWizard {
    /// A wizard for sharing `node`, starting on the node-only row.
    pub(crate) fn new(node: Node) -> Self {
        Self {
            node,
            selected: 0,
            clicks: ClickSurface::default(),
        }
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
            // Row 1 is "with children", row 0 is the node alone, matching the
            // export wizard's scope step.
            Action::Confirm => Some(TreeAction::BeginShareDestination {
                node: self.node.clone(),
                include_children: self.selected == 1,
            }),
            _ => None,
        }
    }

    /// Applies a click on a row: select it, or on a repeat activate the scope.
    pub(crate) fn handle_click(
        &mut self,
        button: MouseButton,
        x: u16,
        y: u16,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let row = self.clicks.target_at(button, x, y)?;
        if row >= texts::SHARE_SCOPE_OPTIONS.len() {
            return None;
        }
        let click = RowClick::resolve(self.selected, row);
        self.selected = row;
        if click.activated() {
            self.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), state)
        } else {
            None
        }
    }

    /// The panel the renderer drew last frame, or `None` before the first frame.
    /// An outside click closes the wizard, like `Esc`.
    pub(crate) fn panel_rect(&self) -> Option<Rect> {
        self.clicks.panel()
    }

    /// Draws the scope list over `area`.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.clicks.clear();
        let overlay_area = centered_rect(area, area.width / 2, 6);
        self.clicks.set_panel(overlay_area);

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

        let visible = texts::SHARE_SCOPE_OPTIONS.len().min(inner.height as usize);
        record_rows(&mut self.clicks, inner, 0, visible, |i| i);
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
    fn the_node_row_is_chosen_by_default() {
        let (mut wizard, mut state) = wizard();

        match wizard.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::BeginShareDestination {
                node,
                include_children,
            }) => {
                assert_eq!(node.name, "Work");
                assert!(!include_children);
            }
            _ => panic!("expected a share destination request"),
        }
    }

    #[test]
    fn the_with_children_row_is_reachable_by_j() {
        let (mut wizard, mut state) = wizard();
        let _ = wizard.handle_key(key(KeyCode::Char('j')), &mut state);
        match wizard.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::BeginShareDestination {
                include_children, ..
            }) => {
                assert!(include_children);
            }
            _ => panic!("expected a share destination request"),
        }
    }

    #[test]
    fn the_scope_can_be_chosen_by_number() {
        let (mut wizard, mut state) = wizard();
        let _ = wizard.handle_key(key(KeyCode::Char('2')), &mut state);
        match wizard.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::BeginShareDestination {
                include_children, ..
            }) => {
                assert!(include_children);
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
