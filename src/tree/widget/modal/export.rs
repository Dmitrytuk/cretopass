//! The Export modal: a two-step wizard that writes a Node to a plain-text file
//! beside the secret file.
//!
//! Step one picks the scope (this Node only, or with its children); step two
//! picks the format. Confirming the format reports a [`TreeAction::Export`] — the
//! file write and the result alert are the orchestrator's job.

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use super::chrome::centered_rect;
use crate::cursor::selected_span;
use crate::file_handling::export::ExportFormat;
use crate::theming::{Action, ElementStyle, texts};
use crate::tree::node::Node;
use crate::tree::widget::TreeState;

/// Steps of the export wizard, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Pick whether to include the Node's children.
    Scope,
    /// Pick JSON, XML, or CSV.
    Format,
}

/// The Export modal: pick the scope, then the format.
#[derive(Debug)]
pub(crate) struct ExportWizard {
    /// The Node being exported, captured when the wizard opened.
    node: Node,
    /// The current step.
    step: Step,
    /// Whether the exported file includes the Node's children.
    include_children: bool,
    /// Focused row on the current step (0/1 for scope, 0/1/2 for format).
    selected: usize,
}

impl ExportWizard {
    /// A wizard for exporting `node`, starting on the scope step.
    pub(crate) fn new(node: Node) -> Self {
        Self {
            node,
            step: Step::Scope,
            include_children: false,
            selected: 0,
        }
    }

    /// Reads a key: [`TreeAction::Close`] on `Esc`, [`TreeAction::Export`] when
    /// the format is confirmed.
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        match self.step {
            Step::Scope => match Action::from_select_field_type(&key) {
                Action::Cancel => Some(TreeAction::Close),
                Action::NavDown | Action::NavUp => {
                    self.selected = (self.selected + 1) % 2;
                    None
                }
                Action::SelectIndex(i) if i < 2 => {
                    self.selected = i;
                    None
                }
                Action::Confirm => {
                    self.include_children = self.selected == 1;
                    self.step = Step::Format;
                    self.selected = 0;
                    None
                }
                _ => None,
            },
            Step::Format => match Action::from_select_field_type(&key) {
                Action::Cancel => Some(TreeAction::Close),
                Action::NavDown => {
                    self.selected = (self.selected + 1) % 3;
                    None
                }
                Action::NavUp => {
                    self.selected = (self.selected + 2) % 3;
                    None
                }
                Action::SelectIndex(i) if i < 3 => {
                    self.selected = i;
                    None
                }
                Action::Confirm => Some(TreeAction::Export {
                    node: self.node.clone(),
                    include_children: self.include_children,
                    format: self.format(),
                }),
                _ => None,
            },
        }
    }

    /// The format the current selection stands for.
    fn format(&self) -> ExportFormat {
        match self.selected {
            0 => ExportFormat::Json,
            1 => ExportFormat::Xml,
            _ => ExportFormat::Csv,
        }
    }

    /// Draws the active step over `area`.
    pub(crate) fn render(&self, frame: &mut Frame, area: Rect) {
        match self.step {
            Step::Scope => self.render_scope(frame, area),
            Step::Format => self.render_format(frame, area),
        }
    }

    /// Badge and hint for the tree status bar while the wizard is open.
    pub(crate) fn status_parts(&self) -> (&'static str, ElementStyle, &'static str) {
        match self.step {
            Step::Scope => (
                texts::MODE_EXPORT_SELECT,
                ElementStyle::ModeFieldType,
                texts::HINT_EXPORT_NODE_SELECT,
            ),
            Step::Format => (
                texts::MODE_EXPORT_FORMAT,
                ElementStyle::ModeFieldType,
                texts::HINT_EXPORT_NODE_FORMAT,
            ),
        }
    }

    /// Draws the scope step.
    fn render_scope(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = centered_rect(area, area.width / 2, 6);

        let block = Block::default()
            .title(texts::OVERLAY_EXPORT_NODE)
            .borders(Borders::ALL)
            .style(ElementStyle::FieldTypeBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = Vec::new();

        for (i, item) in texts::EXPORT_NODE_OPTIONS.iter().enumerate() {
            lines.push(Line::from(selected_span(item, i == self.selected)));
        }

        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    }

    /// Draws the format step.
    fn render_format(&self, frame: &mut Frame, area: Rect) {
        let overlay_area = centered_rect(area, area.width / 2, 7);

        let block = Block::default()
            .title(texts::EXPORT_SELECT_TITLE)
            .borders(Borders::ALL)
            .style(ElementStyle::FieldTypeBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = Vec::new();

        for (i, item) in texts::EXPORT_TYPE_ITEMS.iter().enumerate() {
            lines.push(Line::from(selected_span(item, i == self.selected)));
        }

        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);
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

    fn wizard() -> (ExportWizard, TreeState) {
        (
            ExportWizard::new(Node::new("Root")),
            TreeState::new(Node::new("Root")),
        )
    }

    #[test]
    fn scope_confirm_advances_to_the_format_step() {
        let (mut wizard, mut state) = wizard();
        let _ = wizard.handle_key(key(KeyCode::Char('j')), &mut state); // "with children"
        assert!(wizard.handle_key(key(KeyCode::Enter), &mut state).is_none());
        assert_eq!(wizard.step, Step::Format);
        assert!(wizard.include_children);
    }

    #[test]
    fn format_confirm_reports_the_export() {
        let (mut wizard, mut state) = wizard();
        let _ = wizard.handle_key(key(KeyCode::Enter), &mut state); // scope -> format
        let _ = wizard.handle_key(key(KeyCode::Char('j')), &mut state); // XML
        match wizard.handle_key(key(KeyCode::Enter), &mut state) {
            Some(TreeAction::Export {
                include_children,
                format,
                ..
            }) => {
                assert!(!include_children);
                assert_eq!(format, ExportFormat::Xml);
            }
            _ => panic!("expected an export action"),
        }
    }

    #[test]
    fn esc_closes_from_either_step() {
        let (mut wizard, mut state) = wizard();
        assert!(matches!(
            wizard.handle_key(key(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
        let _ = wizard.handle_key(key(KeyCode::Enter), &mut state);
        assert!(matches!(
            wizard.handle_key(key(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn status_parts_follow_the_step() {
        let (mut wizard, mut state) = wizard();
        assert_eq!(wizard.status_parts().0, texts::MODE_EXPORT_SELECT);
        let _ = wizard.handle_key(key(KeyCode::Enter), &mut state);
        assert_eq!(wizard.status_parts().0, texts::MODE_EXPORT_FORMAT);
    }
}
