use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use super::state::{FileBrowserMode, FileBrowserState, FileFilter, FocusTarget, NewFilePrefix};
use crate::cursor::cursor_field_spans;
use crate::theming::{ElementStyle, Icon, texts};

const DEFAULT_HEIGHT: u16 = 18;

/// Context lines kept visible above/below the selection while scrolling,
/// similar to Vim/Neovim's `scrolloff`. Capped on short viewports.
const SCROLL_OFFSET: usize = 3;

/// Renders the file browser modal over the given screen area.
pub fn render_file_browser(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let overlay_area = make_overlay(area);

    // Clear overlay area first, then draw block borders on top
    frame.render_widget(Clear, overlay_area);

    let title = match state.mode {
        FileBrowserMode::OpenFile => texts::BROWSER_OPEN,
        FileBrowserMode::CreateFile => texts::BROWSER_CREATE,
        FileBrowserMode::ImportFile => texts::BROWSER_IMPORT,
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(ElementStyle::DetailBlock.style());

    frame.render_widget(&block, overlay_area);

    let inner = block.inner(overlay_area);

    // Split the inner area: path bar, file list, options bar.
    let layout = Layout::vertical([
        Constraint::Length(1), // path bar
        Constraint::Min(1),    // file list
        Constraint::Length(2), // options bar (filter + search + actions)
    ])
    .split(inner);

    render_path_bar(frame, layout[0], state);
    render_file_list(frame, layout[1], state);
    render_options_bar(frame, layout[2], state);

    // Transient prompts are centered popups stacked over the browser:
    // the active input / delete confirmation first, the error on top.
    if state.mode == FileBrowserMode::CreateFile && state.focus == FocusTarget::Input {
        render_input_popup(
            frame,
            overlay_area,
            texts::OVERLAY_NEW_FILE,
            texts::LABEL_NAME_INLINE,
            &state.creating_name,
            state.creating_cursor,
        );
    } else if state.focus == FocusTarget::Rename {
        render_input_popup(
            frame,
            overlay_area,
            texts::OVERLAY_RENAME,
            texts::LABEL_RENAME,
            &state.rename_name,
            state.rename_cursor,
        );
    } else if state.confirm_delete.is_some() {
        render_delete_popup(frame, overlay_area, state);
    } else if state.focus == FocusTarget::Locations {
        render_locations_popup(frame, area, state);
    }

    if state.error.is_some() {
        render_error_popup(frame, overlay_area, state);
    }
}

fn make_overlay(area: Rect) -> Rect {
    let height = DEFAULT_HEIGHT.min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 8,
        y: (area.height.saturating_sub(height)) / 2,
        width: area.width * 6 / 8,
        height,
    }
}

fn render_path_bar(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let path_str = state.current_dir.to_string_lossy();
    let text = Line::from(Span::styled(
        format!(" {}", path_str),
        ElementStyle::FbPathBar.style(),
    ));
    frame.render_widget(Paragraph::new(text), area);
}

/// Returns the first visible item for a selection at `selected_pos`.
///
/// Keeps up to [`SCROLL_OFFSET`] rows of context below the cursor while
/// scrolling down (and above it while scrolling up). At the end of the list
/// the last item is allowed to reach the bottom row, so the margin is clamped
/// to `item_count - viewport_height`; short lists never scroll.
fn scroll_start(selected_pos: usize, item_count: usize, viewport_height: usize) -> usize {
    if viewport_height == 0 {
        return 0;
    }

    let margin = SCROLL_OFFSET.min(viewport_height.saturating_sub(1) / 2);
    let max_row = viewport_height.saturating_sub(1).saturating_sub(margin);
    let max_scroll = item_count.saturating_sub(viewport_height);

    selected_pos.saturating_sub(max_row).min(max_scroll)
}

fn render_file_list(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let indices = state.filtered_indices();
    let list_height = area.height as usize;

    // Determine scroll position
    let sel_pos = indices
        .iter()
        .position(|&i| i == state.selected)
        .unwrap_or(0);

    let scroll = scroll_start(sel_pos, indices.len(), list_height);

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
            } else if entry.is_age_file {
                ElementStyle::FbAgeFileName.style()
            } else {
                ElementStyle::FbRegularFileName.style()
            };

            let name_style = if is_selected {
                ElementStyle::FbSelectionOverlay.style()
            } else {
                name_style
            };

            let marker = if entry.is_age_file
                && !entry.is_dir
                && state.mode != FileBrowserMode::ImportFile
            {
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

fn render_options_bar(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let filter_text = match state.filter {
        FileFilter::All => texts::FB_FILTER_ALL,
        FileFilter::PwdKeeperOnly => texts::FB_FILTER_PWDKEEPER,
    };

    let hint_style = ElementStyle::FbHintText.style();
    let search_style = ElementStyle::FbActiveSearch.style();

    // Row 1: filter / search / hidden indicator.
    let mut first = vec![
        Span::styled(format!(" Filter: {} ", filter_text), hint_style),
        Span::styled(texts::FB_HINT_FILTER, hint_style),
    ];

    if state.search_active {
        first.push(Span::styled(
            format!(" Search: {}█", state.search_query),
            search_style,
        ));
    } else {
        first.push(Span::styled(texts::FB_HINT_SEARCH, hint_style));
    }

    first.push(Span::styled(
        format!(
            " {} ",
            if state.show_hidden {
                texts::FB_HIDDEN_ON
            } else {
                texts::FB_HIDDEN_OFF
            }
        ),
        hint_style,
    ));

    // Row 2: contextual actions, replaced by the n-prefix prompt.
    let mut second = vec![
        Span::styled(texts::FB_HINT_DISKS, hint_style),
        Span::styled(texts::FB_HINT_HIDDEN, hint_style),
    ];

    if state.mode == FileBrowserMode::CreateFile {
        second.push(Span::styled(texts::FB_HINT_NEW, hint_style));
    }

    // Show rename/delete hints when a PWD-marked file is selected
    if let Some(entry) = state.entries.get(state.selected)
        && entry.is_age_file
        && !entry.is_dir
        && state.mode != FileBrowserMode::ImportFile
    {
        second.push(Span::styled(texts::FB_HINT_RENAME, hint_style));
        second.push(Span::styled(texts::FB_HINT_DELETE, hint_style));
    }

    if state.mode == FileBrowserMode::CreateFile && state.focus == FocusTarget::Input {
        second.push(Span::styled(texts::FB_HINT_LIST, hint_style));
    }

    let mut lines = vec![Line::from(first)];
    if state.pending == NewFilePrefix::Pending {
        lines.push(Line::from(Span::styled(
            texts::FB_PROMPT_NEW,
            ElementStyle::FbActiveSearch.style(),
        )));
    } else {
        lines.push(Line::from(second));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

fn render_locations_popup(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let row_count = state.locations.len().max(1) as u16;
    let height = row_count.saturating_add(3); // borders + rows + hint
    let popup_area = centered_popup_sized(area, height, 3, 4, 40);

    let block = Block::default()
        .title(texts::OVERLAY_LOCATIONS)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(popup_area);
    let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(inner);
    let list_height = layout[0].height as usize;

    let scroll = scroll_start(state.location_selected, state.locations.len(), list_height);

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

    if rows.is_empty() {
        rows.push(Line::from(Span::styled(
            texts::EMPTY_DIR,
            ElementStyle::FbEmptyText.style(),
        )));
    }

    let hint = Line::from(Span::styled(
        texts::FB_HINT_LOCATIONS,
        ElementStyle::FbHintText.style(),
    ));

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(rows)), layout[0]);
    frame.render_widget(Paragraph::new(hint), layout[1]);
}

fn render_input_popup(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    label: &str,
    text: &str,
    cursor: usize,
) {
    let popup_area = centered_popup(area, 5);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(popup_area);
    let lines = vec![
        Line::from(""),
        Line::from(cursor_field_spans(label, text, cursor, true)),
        Line::from(Span::styled(
            format!("{}  {}", texts::FB_HINT_CONFIRM, texts::FB_HINT_CANCEL),
            ElementStyle::GrayHint.style(),
        )),
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_delete_popup(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let Some(ref path_str) = state.confirm_delete else {
        return;
    };

    let filename = std::path::Path::new(path_str)
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or(std::borrow::Cow::Borrowed(path_str));

    let popup_area = centered_popup(area, 5);
    let block = Block::default()
        .title(texts::OVERLAY_CONFIRM_DELETE)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let inner = block.inner(popup_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" Delete {filename}? "),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(Span::styled(
            texts::FB_DELETE_CONFIRM_SUFFIX,
            ElementStyle::DismissHint.style(),
        )),
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_error_popup(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let Some(ref err) = state.error else {
        return;
    };

    let popup_area = centered_popup(area, 5);
    let block = Block::default()
        .title(texts::HOME_ALERT)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeAlertBlock.style());

    let inner = block.inner(popup_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(" {err}"),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(Span::styled(
            texts::HINT_ALERT,
            ElementStyle::DismissHint.style(),
        )),
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Builds a small dialog rect centered on `area`, clamped to fit.
fn centered_popup(area: Rect, height: u16) -> Rect {
    centered_popup_sized(area, height, 2, 3, 30)
}

/// Builds a centered dialog with a configurable width ratio and minimum width.
fn centered_popup_sized(
    area: Rect,
    height: u16,
    numerator: u16,
    denominator: u16,
    min_width: u16,
) -> Rect {
    let height = height.min(area.height.saturating_sub(2));
    let width = (area.width.saturating_mul(numerator) / denominator)
        .max(min_width)
        .min(area.width.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::file_browser::{FileBrowserSession, Location};
    use crate::file_handling;

    /// Helper to create a terminal for testing.
    fn make_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(80, 24)).unwrap()
    }

    fn new_state(mode: FileBrowserMode) -> FileBrowserState {
        FileBrowserState::new(mode, &FileBrowserSession::default())
    }

    /// Helper to populate state with entries from /tmp (known to exist).
    fn populate_tmp(state: &mut FileBrowserState) {
        state.current_dir = std::path::PathBuf::from("/tmp");
        state.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap();
        state.selected = 0;
    }

    /// Renders and returns the full buffer content as a single string.
    fn render_to_text(state: &FileBrowserState) -> String {
        let mut terminal = make_terminal();
        terminal
            .draw(|f| {
                let area = f.area();
                render_file_browser(f, area, state);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
    }

    #[test]
    fn test_render_open_file_shows_path_bar() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            text.contains("/tmp"),
            "path bar should show '/tmp', got:\n{text}"
        );
    }

    #[test]
    fn test_render_open_file_shows_entries() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        // At least one entry name should appear in the output
        let has_entry = state.entries.iter().any(|e| text.contains(&e.name));
        assert!(has_entry, "no entry name found in rendered output:\n{text}");
    }

    #[test]
    fn test_render_open_file_shows_filter_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            text.contains("Filter:"),
            "options bar should show filter indicator:\n{text}"
        );
        assert!(
            text.contains("[f]"),
            "options bar should show filter toggle hint:\n{text}"
        );
    }

    #[test]
    fn test_render_open_file_shows_search_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            text.contains("[/]search") || text.contains("Search:"),
            "options bar should have search hint:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_no_name_input_when_focus_is_list() {
        // In CreateFile mode but focus is on list → no name input area
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::List;
        let text = render_to_text(&state);
        // Title should be "Create New Secret File"
        assert!(
            text.contains("Create New Secret File"),
            "title mismatch:\n{text}"
        );
        // n hint should mention the new file / new folder prefix
        assert!(
            text.contains("[n]new"),
            "should show n hint for new file/folder:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_shows_name_input_when_focused() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let text = render_to_text(&state);
        assert!(
            text.contains("Name:"),
            "should show Name: input when focused:\n{text}"
        );
        assert!(
            text.contains("Enter:confirm"),
            "should show Enter:confirm hint:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_name_input_is_popup() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let text = render_to_text(&state);
        assert!(
            text.contains("New File"),
            "name input should render as a titled popup:\n{text}"
        );
    }

    #[test]
    fn test_render_rename_input_is_popup() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Rename;
        state.rename_name = "renamed.age".to_string();
        state.rename_cursor = state.rename_name.len();
        let text = render_to_text(&state);
        assert!(
            text.contains("Rename"),
            "rename input should render as a titled popup:\n{text}"
        );
        assert!(
            text.contains("renamed.age"),
            "popup should show the name being edited:\n{text}"
        );
    }

    #[test]
    fn test_render_delete_confirm_is_popup() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.confirm_delete = Some("/tmp/secret.age".to_string());
        let text = render_to_text(&state);
        assert!(
            text.contains("Confirm Delete"),
            "delete confirmation should render as a titled popup:\n{text}"
        );
        assert!(
            text.contains("Delete secret.age?"),
            "popup should name the file to delete:\n{text}"
        );
    }

    #[test]
    fn test_render_error_is_popup() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.error = Some("Permission denied".to_string());
        let text = render_to_text(&state);
        assert!(
            text.contains("Alert"),
            "error should render as a titled popup:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_with_partial_name() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        state.creating_name = "myvault.age".to_string();
        state.creating_cursor = 5;
        let text = render_to_text(&state);
        assert!(
            text.contains("myvault"),
            "should show the typed name:\n{text}"
        );
    }

    #[test]
    fn test_render_search_active_shows_query() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.search_active = true;
        state.search_query = "test".to_string();
        let text = render_to_text(&state);
        assert!(
            text.contains("Search: test"),
            "should show search query:\n{text}"
        );
    }

    #[test]
    fn test_render_error_bar_shows_message() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.error = Some("Permission denied".to_string());
        let text = render_to_text(&state);
        assert!(
            text.contains("Permission denied"),
            "should show error text:\n{text}"
        );
    }

    #[test]
    fn test_render_empty_filtered_list_shows_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        // Use an empty directory if possible, or filter with PwdKeeperOnly
        // and search for something that won't match
        state.current_dir = std::path::PathBuf::from("/tmp");
        // Create a known empty directory
        let empty_dir = std::path::Path::new("/tmp/__test_empty_xyz__");
        let _ = std::fs::create_dir_all(empty_dir);
        state.entries = file_handling::list_dir(empty_dir).unwrap();
        state.filter = FileFilter::PwdKeeperOnly;
        let text = render_to_text(&state);
        // Searching for something non-existent
        assert!(
            text.contains("No matching files") || text.contains("(empty directory)"),
            "should show empty state message:\n{text}"
        );
        let _ = std::fs::remove_dir(empty_dir);
    }

    #[test]
    fn test_render_new_hint_shown_in_create_mode() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            text.contains("[n]new"),
            "should show new file/folder hint in CreateFile mode:\n{text}"
        );
    }

    #[test]
    fn test_render_new_hint_hidden_in_open_mode() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            !text.contains("[n]new"),
            "should NOT show new file/folder hint in OpenFile mode:\n{text}"
        );
    }

    #[test]
    fn test_render_n_prefix_prompt_when_pending() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.pending = NewFilePrefix::Pending;
        let text = render_to_text(&state);
        assert!(
            text.contains("n:new file") && text.contains("N:new folder"),
            "should show the n-prefix prompt while pending:\n{text}"
        );
    }

    #[test]
    fn test_render_dirs_marked_with_arrow_icon() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        // The selected entry should show ▸ prefix
        let has_arrow = text.contains('▸');
        assert!(has_arrow, "should have selection arrow (▸):\n{text}");
    }

    #[test]
    fn test_render_pwd_marker_shown_for_age_files() {
        let dir = std::path::Path::new("/tmp/__test_render_pwd__");
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();

        // Create an age file
        let age_path = dir.join("myvault.age");
        let data = [b"age-encrypt".as_slice(), &[0u8, 1, 2, 3]].concat();
        std::fs::write(&age_path, &data).unwrap();

        // Create a regular file too
        let txt_path = dir.join("notes.txt");
        std::fs::write(&txt_path, b"hello").unwrap();

        let mut state = new_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.to_path_buf();
        state.entries = file_handling::list_dir(dir).unwrap();
        state.selected = state
            .entries
            .iter()
            .position(|e| e.name == "myvault.age")
            .unwrap_or(0);

        let text = render_to_text(&state);

        assert!(
            text.contains("[PWD]"),
            "should show [PWD] marker for age files:\n{text}"
        );
        assert!(
            text.contains("myvault.age"),
            "should show age filename:\n{text}"
        );
        assert!(
            text.contains("notes.txt"),
            "should show regular filename:\n{text}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    // ── Options bar: hidden + disks hints ──

    #[test]
    fn test_render_footer_shows_hidden_state_and_hints() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);

        let text = render_to_text(&state);
        assert!(
            text.contains("Hidden: off"),
            "hidden indicator should start off:\n{text}"
        );
        assert!(
            text.contains("[D]disks"),
            "should advertise the locations overlay:\n{text}"
        );
        assert!(
            text.contains("[Ctrl+h]hidden"),
            "should advertise the hidden toggle:\n{text}"
        );

        state.show_hidden = true;
        let text = render_to_text(&state);
        assert!(
            text.contains("Hidden: on"),
            "hidden indicator should reflect the toggle:\n{text}"
        );
    }

    // ── Locations overlay ──

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

        let text = render_to_text(&state);

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
        assert!(
            text.contains("Enter:open"),
            "overlay should show its key hints:\n{text}"
        );
    }

    #[test]
    fn test_render_locations_overlay_without_rows_shows_empty_state() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.locations.clear();
        state.focus = FocusTarget::Locations;

        let text = render_to_text(&state);

        assert!(
            text.contains("Locations"),
            "overlay should still render:\n{text}"
        );
    }

    // ── Scroll margin (scrolloff) ──

    #[test]
    fn test_scroll_start_keeps_top_context() {
        // Height 10 with a 3-row margin: the cursor may sit on row 6 before
        // the viewport starts following it.
        assert_eq!(scroll_start(0, 100, 10), 0);
        assert_eq!(scroll_start(6, 100, 10), 0);
        assert_eq!(scroll_start(7, 100, 10), 1);
        assert_eq!(scroll_start(8, 100, 10), 2);
    }

    #[test]
    fn test_scroll_start_keeps_three_rows_below_cursor() {
        let height = 10;
        let selected = 50;
        let scroll = scroll_start(selected, 100, height);

        assert_eq!(scroll, 44);
        assert_eq!(
            selected - scroll,
            6,
            "cursor should stay 3 rows above the bottom"
        );
    }

    #[test]
    fn test_scroll_start_clamps_at_end_of_list() {
        // Last item sits on the bottom row instead of leaving blank rows.
        assert_eq!(scroll_start(99, 100, 10), 90);
        assert_eq!(scroll_start(98, 100, 10), 90);
        assert_eq!(scroll_start(97, 100, 10), 90);
        assert_eq!(scroll_start(96, 100, 10), 90);
        assert_eq!(scroll_start(95, 100, 10), 89);
    }

    #[test]
    fn test_scroll_start_short_list_never_scrolls() {
        assert_eq!(scroll_start(4, 5, 10), 0);
        assert_eq!(scroll_start(0, 0, 10), 0);
    }

    #[test]
    fn test_scroll_start_tiny_viewport_keeps_cursor_visible() {
        assert_eq!(scroll_start(0, 100, 0), 0);
        // Height 1: the cursor is the only row.
        assert_eq!(scroll_start(5, 100, 1), 5);
        // Height 2–3: margin collapses to 0, cursor stays on the last row.
        assert_eq!(scroll_start(5, 100, 2), 4);
        assert_eq!(scroll_start(5, 100, 3), 4);
        // Height 4: margin caps at 1, cursor stays one row above the bottom.
        assert_eq!(scroll_start(5, 100, 4), 3);
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

        let text = render_to_text(&state);

        // The 13-row list keeps rows 13–15 visible below the cursor.
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

        let text = render_to_text(&state);

        assert!(
            text.contains("loc-20") && text.contains("loc-23"),
            "locations below the cursor should remain visible:\n{text}"
        );
        assert!(
            !text.contains("loc-24"),
            "only the 3-row margin should be visible below the cursor:\n{text}"
        );
    }
}
