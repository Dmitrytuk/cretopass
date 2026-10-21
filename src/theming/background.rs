//! The app-owned **background fill** shared by every screen.
//!
//! Each screen calls [`paint_background`] once, over its full area, before it
//! draws anything else. The fill is the palette's [`BASE_BG`](crate::theming::palette::BASE_BG)
//! — the canvas the [`SURFACE`](crate::theming::palette::SURFACE) panels sit on
//! (see [ADR 0018](../../../../docs/adr/0018-app-owned-background-fill.md)) — so
//! no cell falls through to the terminal's own background and the interface
//! renders identically in every terminal: the same promise the palettes make for
//! the foreground.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Block;

use crate::theming::ElementStyle;

/// Paints the app background across `area`.
///
/// Call it once at the top of a screen's render, before any other widget.
/// Widgets drawn afterwards overwrite only the cells they own, so every cell the
/// app does not otherwise style keeps [`BASE_BG`](crate::theming::palette::BASE_BG)
/// instead of the terminal's background.
pub fn paint_background(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Block::default().style(ElementStyle::AppBackground.style()),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::theming::palette;

    #[test]
    fn fills_the_area_with_the_canvas_colour() {
        let mut terminal = Terminal::new(TestBackend::new(20, 5)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                paint_background(frame, area);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        assert!(
            buffer
                .content
                .iter()
                .all(|cell| cell.style().bg == Some(palette::BASE_BG)),
            "every cell should be filled with BASE_BG"
        );
    }
}
