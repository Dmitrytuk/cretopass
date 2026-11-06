//! The Detail modal's Browse screen: the selected Node's ExtraFields.
//!
//! Browsing is read-only apart from two effects: `y`/`c` copies the highlighted
//! field's value ([`TreeAction::CopyToClipboard`]) and `i`/`Enter` opens the
//! Form. A click selects a row, and a repeat click (or a right click) acts on it
//! the way its key would.

use crossterm::event::{KeyCode, KeyEvent, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::super::chrome::make_overlay_area;
use super::{Detail, DetailHit, TreeAction, plain_key};
use crate::cursor::selected_span;
use crate::theming::{Action, ElementStyle};
use crate::tree::widget::{FormState, TreeState};

impl Detail {
    /// Keys while browsing the Node's extra fields.
    pub(super) fn handle_browse(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let field_count = state.current_node_ref().map_or(0, |n| n.extra_fields.len());

        match Action::from_detail(&key) {
            Action::Cancel => Some(TreeAction::Close),
            Action::NavDown => {
                self.selected = (self.selected + 1).min(field_count.saturating_sub(1));
                None
            }
            Action::NavUp => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            Action::NextField if field_count > 0 => {
                self.selected = (self.selected + 1) % field_count;
                None
            }
            Action::PrevField if field_count > 0 => {
                self.selected = (self.selected + field_count - 1) % field_count;
                None
            }
            Action::CopyField => {
                // With nothing selected (a node that has no extra fields) `y` is
                // a no-op: never claim success without a value to copy.
                let text = state.current_node_ref().and_then(|node| {
                    node.extra_fields
                        .get(self.selected)
                        .map(|ef| ef.value.clone())
                })?;
                // An empty value is a silent no-op: writing it would wipe the
                // user's clipboard. The verdict for a real copy is the
                // orchestrator's, shown as a result modal.
                if text.is_empty() {
                    None
                } else {
                    Some(TreeAction::CopyToClipboard { text })
                }
            }
            Action::EditNode => {
                if let Some(node) = state.current_node_ref() {
                    self.form = Some(FormState::new_edit(node));
                }
                None
            }
            _ => None,
        }
    }

    /// A click on a browsed field row: select it, or on a repeat open the Form.
    pub(super) fn click_field(
        &mut self,
        index: usize,
        button: MouseButton,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        let was_selected = self.selected == index;
        self.selected = index;
        match button {
            MouseButton::Left if was_selected => self.handle_key(plain_key(KeyCode::Enter), state),
            MouseButton::Right => self.handle_key(plain_key(KeyCode::Char('y')), state),
            _ => None,
        }
    }

    /// Draws the browsed Node's ExtraFields.
    pub(super) fn render_browse(&mut self, frame: &mut Frame, area: Rect, state: &TreeState) {
        let Some(node) = state.current_node_ref() else {
            return;
        };

        let overlay_area = make_overlay_area(area, node.extra_fields.len(), 3);
        self.clicks.set_panel(overlay_area);

        let block = Block::default()
            .title(format!(" {} ", node.name))
            .borders(Borders::ALL)
            .style(ElementStyle::DetailBlock.style());

        let inner = block.inner(overlay_area);
        let mut lines: Vec<Line> = Vec::new();

        lines.push(Line::from(""));

        for (i, ef) in node.extra_fields.iter().enumerate() {
            let display_val = ef.field_type.display_value(&ef.value);
            let text = format!("{}: {}", ef.field_type.label(), display_val);
            lines.push(Line::from(selected_span(&text, self.selected == i)));
        }

        lines.push(Line::from(""));

        frame.render_widget(Clear, overlay_area);
        frame.render_widget(block, overlay_area);
        frame.render_widget(Paragraph::new(Text::from(lines)), inner);

        // The first line is blank, so field `i` is drawn on `inner.y + 1 + i`.
        // Only rows the card actually shows can be clicked.
        let visible = node
            .extra_fields
            .len()
            .min(inner.height.saturating_sub(1) as usize);
        for i in 0..visible {
            let rect = Rect::new(inner.x, inner.y + 1 + i as u16, inner.width, 1);
            self.clicks.record(rect, DetailHit::Field(i));
            self.clicks
                .record_with(rect, MouseButton::Right, DetailHit::Field(i));
        }
    }
}
