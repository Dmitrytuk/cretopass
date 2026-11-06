//! The browser's centered popups: the Locations picker and the transient
//! input / delete / overwrite / error dialogs.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::cursor::cursor_field_spans;
use crate::file_browser::state::{BrowserHit, FileBrowserState, FocusTarget};
use crate::theming::{ElementStyle, Icon, texts};

use super::chrome::{centered_popup, centered_popup_sized};
use super::list::update_scroll;

pub(super) fn render_locations_popup(
    frame: &mut Frame,
    area: Rect,
    state: &mut FileBrowserState,
) -> Rect {
    // The Home row shows at once; a `Loading…` row holds the disks' place
    // until the background enumeration lands.
    let loading = state.locations_loading();
    let row_count = (state.locations.len() + usize::from(loading)).max(1) as u16;
    let height = row_count.saturating_add(2); // borders + rows
    let popup_area = centered_popup_sized(area, height, 3, 4, 40);

    let block = Block::default()
        .title(texts::OVERLAY_LOCATIONS)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(popup_area);
    let list_height = inner.height as usize;

    update_scroll(
        &mut state.location_scroll,
        state.location_selected,
        state.locations.len(),
        list_height,
    );
    let scroll = state.location_scroll;

    // Record the clickable rows before the borrowing `rows` build below.
    if state.focus == FocusTarget::Locations {
        for (offset, index) in (scroll..scroll + list_height).enumerate() {
            if index >= state.locations.len() {
                break;
            }
            let rect = Rect::new(inner.x, inner.y + offset as u16, inner.width, 1);
            state.clicks.record(rect, BrowserHit::Location(index));
        }
    }

    let label_width = state
        .locations
        .iter()
        .map(|location| location.label.chars().count())
        .max()
        .unwrap_or(0)
        .min(28);

    let mut rows: Vec<Line> = state
        .locations
        .iter()
        .enumerate()
        .skip(scroll)
        .take(list_height)
        .map(|(index, location)| {
            let is_selected = index == state.location_selected;
            let prefix = if is_selected {
                Icon::SelectionArrow.as_str()
            } else {
                Icon::SelectionPad.as_str()
            };
            let label_style = if is_selected {
                ElementStyle::FbSelectionOverlay.style()
            } else {
                ElementStyle::FbDirName.style()
            };
            let detail_style = if is_selected {
                ElementStyle::FbSelectionOverlay.style()
            } else {
                ElementStyle::FbHintText.style()
            };

            Line::from(vec![
                Span::styled(prefix, ElementStyle::FbPrefix.style()),
                Span::styled(
                    format!("{:<width$}", location.label, width = label_width),
                    label_style,
                ),
                Span::styled(format!("  {}", location.detail), detail_style),
            ])
        })
        .collect();

    if loading {
        rows.push(Line::from(Span::styled(
            texts::FB_LOADING,
            ElementStyle::FbEmptyText.style(),
        )));
    } else if rows.is_empty() {
        rows.push(Line::from(Span::styled(
            texts::EMPTY_DIR,
            ElementStyle::FbEmptyText.style(),
        )));
    }

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(rows)), inner);
    popup_area
}
pub(super) fn render_input_popup(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    label: &str,
    text: &str,
    cursor: usize,
) -> Rect {
    let popup_area = centered_popup(area, 4);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(popup_area);
    let lines = vec![
        Line::from(""),
        Line::from(cursor_field_spans(label, text, cursor)),
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    popup_area
}
pub(super) fn render_delete_popup(
    frame: &mut Frame,
    area: Rect,
    state: &FileBrowserState,
) -> Option<Rect> {
    let path_str = state.confirm_delete.as_ref()?;
    let popup_area = centered_popup(area, 4);

    let filename = std::path::Path::new(path_str)
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed(path_str));

    let block = Block::default()
        .title(texts::OVERLAY_CONFIRM_DELETE)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let inner = block.inner(popup_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "{}{}{}",
                texts::FB_CONFIRM_DELETE_START,
                filename,
                texts::FB_CONFIRM_DELETE_END
            ),
            ElementStyle::ConfirmMessage.style(),
        )),
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    Some(popup_area)
}
pub(super) fn render_overwrite_popup(
    frame: &mut Frame,
    area: Rect,
    state: &FileBrowserState,
) -> Option<Rect> {
    let path_str = state.confirm_overwrite.as_ref()?;
    let popup_area = centered_popup(area, 4);

    let filename = std::path::Path::new(path_str)
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed(path_str));

    let block = Block::default()
        .title(texts::OVERLAY_CONFIRM_OVERWRITE)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let inner = block.inner(popup_area);
    // An export overwrites a file; a share writes into a folder that already has
    // something in it.
    let prompt = if state.mode.is_share_destination() {
        format!("{}{}? ", texts::FB_PROMPT_SHARE_FOLDER, filename)
    } else {
        format!(
            "{}{}{}",
            texts::FB_CONFIRM_OVERWRITE_START,
            filename,
            texts::FB_CONFIRM_OVERWRITE_END
        )
    };
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(prompt, ElementStyle::ConfirmMessage.style())),
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    Some(popup_area)
}
pub(super) fn render_error_popup(
    frame: &mut Frame,
    area: Rect,
    state: &FileBrowserState,
) -> Option<Rect> {
    let err = state.error.as_ref()?;
    let popup_area = centered_popup(area, 4);

    let block = Block::default()
        .title(texts::ALERT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::AlertErrorBlock.style());

    let inner = block.inner(popup_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" {err}"),
            ElementStyle::AlertErrorText.style(),
        )),
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
    Some(popup_area)
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{new_state, populate_tmp, render_to_text};
    use crate::file_browser::state::FocusTarget;
    use crate::file_browser::status_parts;
    use crate::file_browser::{FileBrowserMode, Location};
    use crate::theming::texts;

    #[test]
    fn test_render_locations_overlay_lists_home_and_disks() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.locations = vec![
            Location {
                label: "Home".to_string(),
                path: std::path::PathBuf::from("/home/user"),
                detail: "/home/user".to_string(),
                is_home: true,
            },
            Location {
                label: "/mnt/usb".to_string(),
                path: std::path::PathBuf::from("/mnt/usb"),
                detail: "vfat · 1.0 GiB free / 2.0 GiB [USB]".to_string(),
                is_home: false,
            },
        ];
        state.location_selected = 1;
        state.focus = FocusTarget::Locations;

        let text = render_to_text(&mut state);

        assert!(
            text.contains("Locations"),
            "overlay should have a title:\n{text}"
        );
        assert!(
            text.contains("Home") && text.contains("/mnt/usb"),
            "overlay should list Home and the disk:\n{text}"
        );
        assert!(
            text.contains("vfat") && text.contains("[USB]"),
            "overlay should show disk details:\n{text}"
        );

        let (mode, _, hint) = status_parts(&state);
        assert_eq!(mode, texts::MODE_MENU);
        assert!(
            hint.contains("Enter:open"),
            "the Locations overlay hint should be routed to the status bar: {hint}"
        );
    }

    #[test]
    fn test_render_locations_overlay_shows_loading_while_disks_are_pending() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);

        state.begin_locations();
        state.focus = FocusTarget::Locations;

        let text = render_to_text(&mut state);

        assert!(text.contains("Home"), "the Home row shows at once:\n{text}");
        assert!(
            text.contains("Loading"),
            "the disk enumeration shows a loading row:\n{text}"
        );
    }

    #[test]
    fn test_render_locations_overlay_without_rows_shows_empty_state() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.locations.clear();
        state.focus = FocusTarget::Locations;

        let text = render_to_text(&mut state);

        assert!(
            text.contains("Locations"),
            "overlay should still render:\n{text}"
        );
    }
    #[test]
    fn test_render_locations_keeps_scroll_margin() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = new_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.path().to_path_buf();
        state.entries.clear();
        state.locations = (0..30)
            .map(|i| Location {
                label: format!("loc-{i:02}"),
                path: std::path::PathBuf::from(format!("/mnt/loc-{i:02}")),
                detail: String::new(),
                is_home: false,
            })
            .collect();
        state.location_selected = 20;
        state.focus = FocusTarget::Locations;

        let text = render_to_text(&mut state);

        assert!(
            text.contains("loc-20") && text.contains("loc-23"),
            "locations below the cursor should remain visible:\n{text}"
        );
        assert!(
            !text.contains("loc-24"),
            "only the 3-row margin should be visible below the cursor:\n{text}"
        );

        state.location_selected = 19;
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.location_scroll, 4,
            "cursor should move up inside the overlay before scrolling"
        );

        state.location_selected = 7;
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.location_scroll, 4,
            "cursor should reach the overlay top margin"
        );

        state.location_selected = 6;
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.location_scroll, 3,
            "overlay should scroll only after the margin"
        );
    }
}
