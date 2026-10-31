//! Drawing the Tree screen: the bordered tree view and its title bar.

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};

use super::state::TreeState;
use crate::theming::{ElementStyle, Icon};

/// Zero-sized type serving as a namespace for the tree rendering function.
pub struct TreeWidget;

impl TreeWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &TreeState) {
        let base_title = state
            .active_vault_path
            .as_deref()
            .map(|p| {
                if state.display_full_path {
                    format!("{} {} ", crate::theming::texts::TITLE_TREE_PREFIX, p)
                } else {
                    let name = std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or(std::borrow::Cow::Borrowed(p));
                    format!("{} {} ", crate::theming::texts::TITLE_TREE_PREFIX, name)
                }
            })
            .unwrap_or_else(|| crate::theming::texts::TITLE_TREE_DEFAULT.to_string());

        let mut title_spans: Vec<Span> = Vec::new();
        // A blocking operation (e.g. the share write) outranks the autosave
        // status while it runs; both share the same spinner glyph.
        if let Some(status) = state.operation_status.or(state.save_state.title_status()) {
            let mut text = String::new();
            if status.spins() {
                text.push_str(state.spinner.glyph());
                text.push(' ');
            }
            text.push_str(status.label());
            title_spans.push(Span::styled(format!(" {text}  "), status.style().style()));
        }
        title_spans.push(Span::raw(base_title));

        let block = Block::default()
            .title(Line::from(title_spans))
            .borders(Borders::ALL)
            .border_style(ElementStyle::TreeBorder.style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines: Vec<Line> = state
            .visible
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let indent = "  ".repeat(v.depth);
                let icon = if v.is_parent {
                    if v.expanded {
                        Icon::Expanded.as_str()
                    } else {
                        Icon::Collapsed.as_str()
                    }
                } else {
                    Icon::LeafPad.as_str()
                };
                let prefix = if i == state.selected {
                    Icon::SelectionArrow.as_str()
                } else {
                    Icon::SelectionPad.as_str()
                };
                let text = format!("{}{}{}{}", prefix, indent, icon, v.name);

                let is_matched = state.matched.contains(&i);
                let style = if is_matched {
                    ElementStyle::TreeRowMatched.style()
                } else {
                    ElementStyle::TreeRow.style()
                };
                Line::from(Span::styled(text, style))
            })
            .collect();

        let text = Text::from(lines);
        let inner_height = inner.height.max(1) as usize;
        let scroll_row = state
            .selected
            .saturating_sub(inner_height.saturating_sub(1) / 2);
        let paragraph = Paragraph::new(text)
            .style(ElementStyle::TreeRow.style())
            .scroll((scroll_row as u16, 0));
        frame.render_widget(paragraph, inner);
    }
}
