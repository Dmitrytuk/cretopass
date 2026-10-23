//! Shared overlay chrome for the Home modals.
//!
//! Every modal draws a bordered panel on a cleared area, narrow enough that the
//! Home screen stays visible on both sides.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Clear, Paragraph};

/// A centred overlay `height` rows tall, three-fifths of the screen wide.
///
/// The height is clamped so a modal never covers the last screen row.
pub fn make_overlay(area: Rect, height: u16) -> Rect {
    let overlay_height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 5,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width * 3 / 5,
        height: overlay_height,
    }
}

/// Draws `block` into `area` with `content` inside it, clearing whatever the
/// screen painted there first.
pub fn paint_overlay(frame: &mut Frame, area: Rect, block: Block<'_>, content: Paragraph<'_>) {
    let inner = block.inner(area);
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);
    frame.render_widget(content, inner);
}
