//! The file list and its scroll-margin viewport maths.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Paragraph;

use crate::file_browser::state::FileBrowserState;
use crate::theming::{ElementStyle, Icon, texts};

/// Context lines kept visible above/below the selection while scrolling,
/// similar to Vim/Neovim's `scrolloff`. Capped on short viewports.
const SCROLL_OFFSET: usize = 3;
/// Updates the first visible item so the cursor moves inside a 3-row margin
/// before the viewport starts scrolling.
///
/// Unlike a stateless calculation, this keeps the viewport where it is while
/// the selection moves up or down within the margin band. Scrolling only
/// starts when the selection would cross the top or bottom margin.
pub(super) fn update_scroll(
    scroll: &mut usize,
    selected_pos: usize,
    item_count: usize,
    viewport_height: usize,
) {
    if viewport_height == 0 {
        *scroll = 0;
        return;
    }

    let margin = SCROLL_OFFSET.min(viewport_height.saturating_sub(1) / 2);
    let max_scroll = item_count.saturating_sub(viewport_height);

    let top = scroll.saturating_add(margin);
    let bottom = scroll
        .saturating_add(viewport_height)
        .saturating_sub(1)
        .saturating_sub(margin);

    if selected_pos < top {
        *scroll = selected_pos.saturating_sub(margin);
    } else if selected_pos > bottom {
        *scroll = selected_pos
            .saturating_add(margin)
            .saturating_add(1)
            .saturating_sub(viewport_height);
    }

    // At the end of the list the last item may reach the bottom row.
    *scroll = (*scroll).min(max_scroll);
}
pub(super) fn render_file_list(frame: &mut Frame, area: Rect, state: &mut FileBrowserState) {
    let indices = state.filtered_indices();
    let list_height = area.height as usize;

    // Determine scroll position
    let sel_pos = indices
        .iter()
        .position(|&i| i == state.selected)
        .unwrap_or(0);

    update_scroll(&mut state.scroll, sel_pos, indices.len(), list_height);
    let scroll = state.scroll;

    let visible: Vec<Line> = indices
        .iter()
        .skip(scroll)
        .take(list_height)
        .map(|&idx| {
            let entry = &state.entries[idx];
            let is_selected = idx == state.selected;
            let prefix = if is_selected {
                Icon::SelectionArrow.as_str()
            } else {
                Icon::SelectionPad.as_str()
            };

            let name_style = if entry.is_dir {
                ElementStyle::FbDirName.style()
            } else if entry.looks_like_vault {
                ElementStyle::FbAgeFileName.style()
            } else {
                ElementStyle::FbRegularFileName.style()
            };

            let name_style = if is_selected {
                ElementStyle::FbSelectionOverlay.style()
            } else {
                name_style
            };

            let marker =
                if entry.looks_like_vault && !entry.is_dir && state.mode.allows_entry_actions() {
                    Icon::PwdMarker.as_str()
                } else {
                    ""
                };

            let marker_style = if is_selected {
                ElementStyle::FbPwdMarkerSelected.style()
            } else {
                ElementStyle::FbPwdMarker.style()
            };

            Line::from(vec![
                Span::styled(prefix, ElementStyle::FbPrefix.style()),
                Span::styled(marker, marker_style),
                Span::styled(&entry.name, name_style),
            ])
        })
        .collect();

    if visible.is_empty() {
        let empty_msg = if state.search_active || !state.search_query.is_empty() {
            texts::NO_MATCHING_FILES
        } else {
            texts::EMPTY_DIR
        };
        let empty = vec![Line::from(Span::styled(
            empty_msg,
            ElementStyle::FbEmptyText.style(),
        ))];
        frame.render_widget(Paragraph::new(Text::from(empty)), area);
    } else {
        frame.render_widget(Paragraph::new(Text::from(visible)), area);
    }
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{new_state, render_to_text};
    use super::update_scroll;
    use crate::file_browser::FileBrowserMode;
    use crate::file_handling;

    // ── Scroll margin (scrolloff) ──

    #[test]
    fn test_update_scroll_moves_cursor_before_scrolling_down() {
        let mut scroll = 0;

        for selected in 0..=9 {
            update_scroll(&mut scroll, selected, 100, 13);
            assert_eq!(scroll, 0, "selected {selected} should not scroll yet");
        }

        update_scroll(&mut scroll, 10, 100, 13);
        assert_eq!(scroll, 1, "scrolling starts at the bottom margin");
    }

    #[test]
    fn test_update_scroll_moves_cursor_before_scrolling_up() {
        let mut scroll = 0;
        update_scroll(&mut scroll, 10, 100, 13);
        assert_eq!(scroll, 1);

        // Moving up keeps the viewport in place until the top margin.
        for selected in (4..=9).rev() {
            update_scroll(&mut scroll, selected, 100, 13);
            assert_eq!(
                scroll, 1,
                "selected {selected} should move within the viewport"
            );
        }

        update_scroll(&mut scroll, 3, 100, 13);
        assert_eq!(
            scroll, 0,
            "the viewport follows once the top margin is reached"
        );
    }

    #[test]
    fn test_update_scroll_clamps_at_end_of_list() {
        let mut scroll = 0;
        update_scroll(&mut scroll, 99, 100, 13);
        assert_eq!(scroll, 87);

        // The cursor moves up from the last row before the viewport scrolls.
        for selected in (90..=98).rev() {
            update_scroll(&mut scroll, selected, 100, 13);
            assert_eq!(
                scroll, 87,
                "selected {selected} should remain in the final viewport"
            );
        }

        update_scroll(&mut scroll, 89, 100, 13);
        assert_eq!(scroll, 86);
    }

    #[test]
    fn test_update_scroll_jump_brings_selection_into_margin() {
        let mut scroll = 0;
        update_scroll(&mut scroll, 50, 100, 13);

        assert_eq!(scroll, 41);
        assert_eq!(50 - scroll, 9, "jump lands on the bottom margin");

        update_scroll(&mut scroll, 0, 100, 13);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn test_update_scroll_short_list_never_scrolls() {
        let mut scroll = 0;
        update_scroll(&mut scroll, 4, 5, 13);
        assert_eq!(scroll, 0);

        update_scroll(&mut scroll, 0, 0, 13);
        assert_eq!(scroll, 0);
    }

    #[test]
    fn test_update_scroll_tiny_viewport_keeps_cursor_visible() {
        let mut scroll = 0;
        update_scroll(&mut scroll, 0, 100, 0);
        assert_eq!(scroll, 0);

        // Height 1: the cursor is the only row.
        scroll = 0;
        update_scroll(&mut scroll, 5, 100, 1);
        assert_eq!(scroll, 5);

        // Height 2–3: margin collapses to 0, cursor stays on the last row.
        scroll = 0;
        update_scroll(&mut scroll, 5, 100, 2);
        assert_eq!(scroll, 4);
        scroll = 0;
        update_scroll(&mut scroll, 5, 100, 3);
        assert_eq!(scroll, 4);

        // Height 4: margin caps at 1, cursor stays one row above the bottom.
        scroll = 0;
        update_scroll(&mut scroll, 5, 100, 4);
        assert_eq!(scroll, 3);
    }
    #[test]
    fn test_render_file_list_keeps_scroll_margin() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..30 {
            std::fs::write(dir.path().join(format!("file{i:02}.txt")), b"x").unwrap();
        }

        let mut state = new_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries = file_handling::list_dir(dir.path()).unwrap();
        state.selected = 12;

        let text = render_to_text(&mut state);

        // The 15-row list keeps the 3-row margin: rows 13–15 stay visible
        // below the cursor (viewport 1..15) while file16 falls outside.
        assert!(
            text.contains("file13.txt") && text.contains("file15.txt"),
            "entries below the cursor should remain visible:\n{text}"
        );
        assert!(
            !text.contains("file16.txt"),
            "only the 3-row margin should be visible below the cursor:\n{text}"
        );
    }

    #[test]
    fn test_render_file_list_moves_cursor_before_scrolling_up() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..30 {
            std::fs::write(dir.path().join(format!("file{i:02}.txt")), b"x").unwrap();
        }

        let mut state = new_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries = file_handling::list_dir(dir.path()).unwrap();

        // The 15-row viewport reaches its bottom margin at row 12, so the
        // first render scrolls once (viewport 0..14 → 1..15).
        state.selected = 12;
        let _ = render_to_text(&mut state);
        assert_eq!(state.scroll, 1);

        state.selected = 11;
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.scroll, 1,
            "cursor should move up inside the viewport before scrolling"
        );

        state.selected = 4;
        let _ = render_to_text(&mut state);
        assert_eq!(state.scroll, 1, "cursor should reach the top margin");

        state.selected = 3;
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.scroll, 0,
            "viewport should follow only after the margin"
        );
    }
}
