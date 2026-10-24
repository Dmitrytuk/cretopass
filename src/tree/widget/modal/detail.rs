//! The Detail modal: a selected Node's ExtraFields.
//!
//! Rows can be navigated and copied to the system clipboard; `e` opens the Form
//! Popup. Copy is the modal's only I/O, reported as [`TreeAction::CopyToClipboard`].

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::TreeAction;
use super::chrome::make_overlay_area;
use crate::cursor::selected_span;
use crate::theming::{Action, ElementStyle, texts};
use crate::tree::widget::TreeState;

/// The Detail modal: browse a selected Node's ExtraFields, copy a value, or edit.
#[derive(Debug, Default)]
pub struct Detail {
    /// Highlighted extra-field row.
    selected: usize,
    /// Copy-confirmation alert text, if any. The next key dismisses it.
    alert: Option<String>,
}

impl Detail {
    /// Reads a key, updating the browse state and reporting what the user asked
    /// for: [`TreeAction::Close`] on `Esc`, [`TreeAction::CopyToClipboard`] when a
    /// non-empty value was copied, [`TreeAction::OpenEdit`] on `e`.
    pub fn handle_key(&mut self, key: KeyEvent, state: &mut TreeState) -> Option<TreeAction> {
        // A copy alert consumes the next key and clears itself.
        if self.alert.is_some() {
            self.alert = None;
            return None;
        }

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
                let text = state.current_node_ref().and_then(|node| {
                    node.extra_fields
                        .get(self.selected)
                        .map(|ef| ef.value.clone())
                });
                self.alert = Some(texts::COPY_CHECK.to_string());
                match text {
                    Some(text) if !text.is_empty() => Some(TreeAction::CopyToClipboard { text }),
                    _ => None,
                }
            }
            Action::EditNode => state.current_node_ref().map(|_| TreeAction::OpenEdit),
            _ => None,
        }
    }

    /// Draws the Node's ExtraFields, and the copy alert if it is armed, over `area`.
    pub fn render(&self, frame: &mut Frame, area: Rect, state: &TreeState) {
        let Some(node) = state.current_node_ref() else {
            return;
        };

        let overlay_area = make_overlay_area(area, node.extra_fields.len(), 3);

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

        if self.alert.is_some() {
            render_copy_alert(frame, area);
        }
    }

    /// Badge and hint for the tree status bar while the modal is open.
    pub fn status_parts(&self) -> (&'static str, ElementStyle, &'static str) {
        (
            texts::MODE_DETAIL,
            ElementStyle::ModeDetail,
            if self.alert.is_some() {
                texts::HINT_DETAIL_ALERT
            } else {
                texts::HINT_DETAIL
            },
        )
    }
}

/// Draws the transient "copied" confirmation over `area`.
fn render_copy_alert(frame: &mut Frame, area: Rect) {
    let overlay_area = Rect {
        x: area.width / 3,
        y: area.height / 2 - 2,
        width: area.width / 3,
        height: 4,
    };

    let block = Block::default()
        .title(texts::OVERLAY_COPIED)
        .borders(Borders::ALL)
        .style(ElementStyle::CopiedBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::COPY_CHECK,
            ElementStyle::CopiedText.style(),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::Node;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// A tree whose root has three extra fields, opened in the Detail modal.
    fn detail_with_fields() -> (Detail, TreeState) {
        let mut node = Node::new("Root");
        for value in ["a", "b", "c"] {
            node.extra_fields.push(crate::tree::node::ExtraField {
                field_type: crate::tree::node::FieldType::Login,
                value: value.to_string(),
            });
        }
        (Detail::default(), TreeState::new(node))
    }

    #[test]
    fn tab_wraps_forward_and_backtab_wraps_backward() {
        let (mut detail, mut state) = detail_with_fields();
        detail.handle_key(key(KeyCode::Tab), &mut state);
        assert_eq!(detail.selected, 1);
        detail.handle_key(key(KeyCode::Tab), &mut state);
        detail.handle_key(key(KeyCode::Tab), &mut state);
        assert_eq!(detail.selected, 0); // wrapped past the last field

        detail.handle_key(key(KeyCode::BackTab), &mut state);
        assert_eq!(detail.selected, 2); // wrapped past the first field
    }

    #[test]
    fn nav_down_clamps_at_last_field() {
        let (mut detail, mut state) = detail_with_fields();
        for _ in 0..5 {
            detail.handle_key(key(KeyCode::Char('j')), &mut state);
        }
        assert_eq!(detail.selected, 2);
    }

    #[test]
    fn nav_up_clamps_at_first_field() {
        let (mut detail, mut state) = detail_with_fields();
        detail.handle_key(key(KeyCode::Char('k')), &mut state);
        assert_eq!(detail.selected, 0);
    }

    #[test]
    fn tab_is_safe_without_fields() {
        let mut detail = Detail::default();
        let mut state = TreeState::new(Node::new("Root"));
        detail.handle_key(key(KeyCode::Tab), &mut state);
        detail.handle_key(key(KeyCode::BackTab), &mut state);
        assert_eq!(detail.selected, 0);
    }

    #[test]
    fn esc_asks_to_close() {
        let (mut detail, mut state) = detail_with_fields();
        assert!(matches!(
            detail.handle_key(key(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn i_asks_to_open_the_edit_form() {
        let (mut detail, mut state) = detail_with_fields();
        assert!(matches!(
            detail.handle_key(key(KeyCode::Char('i')), &mut state),
            Some(TreeAction::OpenEdit)
        ));
    }

    #[test]
    fn y_copies_the_selected_value_and_alerts() {
        let (mut detail, mut state) = detail_with_fields();
        match detail.handle_key(key(KeyCode::Char('y')), &mut state) {
            Some(TreeAction::CopyToClipboard { text }) => assert_eq!(text, "a"),
            _ => panic!("expected a clipboard copy"),
        }
        assert_eq!(detail.alert.as_deref(), Some(texts::COPY_CHECK));
    }

    #[test]
    fn an_empty_value_skips_the_clipboard_but_still_alerts() {
        let mut node = Node::new("Root");
        node.extra_fields.push(crate::tree::node::ExtraField {
            field_type: crate::tree::node::FieldType::Login,
            value: String::new(),
        });
        let mut detail = Detail::default();
        let mut state = TreeState::new(node);
        assert!(
            detail
                .handle_key(key(KeyCode::Char('y')), &mut state)
                .is_none()
        );
        assert_eq!(detail.alert.as_deref(), Some(texts::COPY_CHECK));
    }

    #[test]
    fn the_next_key_after_a_copy_alert_is_consumed() {
        let (mut detail, mut state) = detail_with_fields();
        detail.handle_key(key(KeyCode::Char('y')), &mut state);
        assert!(detail.handle_key(key(KeyCode::Esc), &mut state).is_none());
        assert!(detail.alert.is_none());
    }

    #[test]
    fn status_parts_switch_hint_while_the_alert_is_armed() {
        let (mut detail, mut state) = detail_with_fields();
        assert_eq!(detail.status_parts().2, texts::HINT_DETAIL);
        detail.handle_key(key(KeyCode::Char('y')), &mut state);
        assert_eq!(detail.status_parts().2, texts::HINT_DETAIL_ALERT);
    }
}
