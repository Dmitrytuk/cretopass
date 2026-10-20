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
///
/// The renderer also maintains the list viewport offsets so the cursor moves
/// inside the scroll margin before the viewport follows it.
pub fn render_file_browser(frame: &mut Frame, area: Rect, state: &mut FileBrowserState) {
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

    // Split the inner area into the path bar and the file list. The status bar
    // is not drawn here: the parent window (Home) owns it and renders the File
    // Browser's mode badge and hints through `status_parts`.
    let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);

    render_path_bar(frame, layout[0], state);
    render_file_list(frame, layout[1], state);

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
        // Centred on the full frame so the disk list has room. `centered_popup_sized`
        // keeps the popup off the bottom row, which is where the parent window's
        // status bar lives, so the hint stays visible.
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

/// Updates the first visible item so the cursor moves inside a 3-row margin
/// before the viewport starts scrolling.
///
/// Unlike a stateless calculation, this keeps the viewport where it is while
/// the selection moves up or down within the margin band. Scrolling only
/// starts when the selection would cross the top or bottom margin.
fn update_scroll(
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

fn render_file_list(frame: &mut Frame, area: Rect, state: &mut FileBrowserState) {
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

/// Maps the browser's mode and active overlay to the unified status-bar parts:
/// `(mode label, badge style, context hint)`.
///
/// The File Browser does not draw a status bar of its own; the parent window
/// (Home) calls this to render the browser's badge and hint in its status line.
///
/// Transient overlays (error, delete confirmation, Locations picker, rename /
/// name input) capture keys, so they own the hint while shown. The list hint is
/// otherwise dynamic: while searching it is the query prompt, while the `n`
/// prefix is armed it is the new-file prompt, and otherwise it is the set of
/// advertised keys.
///
/// The badge is the browser mode (`OPEN`/`CREATE`/`IMPORT`) unless the
/// PwdKeeper-only filter is active, in which case it is `PWD` in every mode.
pub(crate) fn status_parts(state: &FileBrowserState) -> (&'static str, ElementStyle, String) {
    // Overlays are drawn on top of the list and route keys themselves.
    if state.error.is_some() {
        return ("", ElementStyle::ModeNormal, texts::HINT_ALERT.to_string());
    }
    if state.confirm_delete.is_some() {
        return (
            texts::MODE_DELETE,
            ElementStyle::ModeConfirmDelete,
            texts::FB_HINT_DELETE_CONFIRM.to_string(),
        );
    }
    if state.focus == FocusTarget::Locations {
        return (
            texts::MODE_MENU,
            ElementStyle::ModeNormal,
            texts::FB_HINT_LOCATIONS.to_string(),
        );
    }
    if state.focus == FocusTarget::Rename {
        return (texts::MODE_EDIT, ElementStyle::ModeEdit, fb_input_hint());
    }

    let (mode_str, mode_style) = match state.mode {
        FileBrowserMode::OpenFile => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_OPEN, ElementStyle::ModeSearch),
        },
        FileBrowserMode::CreateFile => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_CREATE, ElementStyle::ModeEdit),
        },
        FileBrowserMode::ImportFile => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_IMPORT, ElementStyle::ModeSearch),
        },
    };

    let hint = if state.search_active {
        format!("Search: {}█", state.search_query)
    } else if state.pending == NewFilePrefix::Pending {
        texts::FB_PROMPT_NEW.to_string()
    } else if state.focus == FocusTarget::Input {
        fb_input_hint()
    } else {
        fb_hint(state)
    };

    (mode_str, mode_style, hint)
}

/// The confirm/dismiss hint for the rename and new-file name inputs.
fn fb_input_hint() -> String {
    format!("{}  {}", texts::FB_HINT_CONFIRM, texts::FB_HINT_CANCEL)
}

/// Concatenates the key hints advertised by the file list for the current mode
/// and selection. Constants carry their own padding, so a plain `trim` suffices.
fn fb_hint(state: &FileBrowserState) -> String {
    let mut parts = vec![
        texts::FB_HINT_FILTER,
        texts::FB_HINT_SEARCH,
        texts::FB_HINT_DISKS,
        texts::FB_HINT_HIDDEN,
    ];

    if state.mode == FileBrowserMode::CreateFile {
        parts.push(texts::FB_HINT_NEW);
    }

    // Rename/delete hints only when a PWD-marked (age) file is selected.
    if let Some(entry) = state.entries.get(state.selected)
        && entry.is_age_file
        && !entry.is_dir
        && state.mode != FileBrowserMode::ImportFile
    {
        parts.push(texts::FB_HINT_RENAME);
        parts.push(texts::FB_HINT_DELETE);
    }

    parts.concat().trim().to_string()
}

fn render_locations_popup(frame: &mut Frame, area: Rect, state: &mut FileBrowserState) {
    let row_count = state.locations.len().max(1) as u16;
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

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(rows)), inner);
}

fn render_input_popup(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    label: &str,
    text: &str,
    cursor: usize,
) {
    let popup_area = centered_popup(area, 4);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(popup_area);
    let lines = vec![
        Line::from(""),
        Line::from(cursor_field_spans(label, text, cursor, true)),
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

    let popup_area = centered_popup(area, 4);
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
    ];

    frame.render_widget(Clear, popup_area);
    frame.render_widget(block, popup_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_error_popup(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let Some(ref err) = state.error else {
        return;
    };

    let popup_area = centered_popup(area, 4);
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
    fn render_to_text(state: &mut FileBrowserState) -> String {
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
        let text = render_to_text(&mut state);
        assert!(
            text.contains("/tmp"),
            "path bar should show '/tmp', got:\n{text}"
        );
    }

    #[test]
    fn test_render_open_file_shows_entries() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&mut state);
        // At least one entry name should appear in the output
        let has_entry = state.entries.iter().any(|e| text.contains(&e.name));
        assert!(has_entry, "no entry name found in rendered output:\n{text}");
    }

    #[test]
    fn status_parts_open_file_shows_open_badge_and_filter_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let (mode, _, hint) = status_parts(&state);
        assert_eq!(
            mode,
            texts::MODE_OPEN,
            "the OPEN badge should lead the hint"
        );
        assert!(
            hint.contains("[f]"),
            "hint should advertise the filter toggle: {hint}"
        );
    }

    #[test]
    fn status_parts_pwd_filter_shows_pwd_badge() {
        for mode in [
            FileBrowserMode::OpenFile,
            FileBrowserMode::CreateFile,
            FileBrowserMode::ImportFile,
        ] {
            let mut state = new_state(mode);
            populate_tmp(&mut state);
            state.filter = FileFilter::PwdKeeperOnly;
            let (mode_label, _, _) = status_parts(&state);
            assert_eq!(
                mode_label,
                texts::MODE_PWD,
                "the PWD badge should replace the mode badge under the PwdKeeper filter in every mode ({mode:?})"
            );
        }
    }

    #[test]
    fn status_parts_mode_badge_shown_without_filter() {
        let mut create = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut create);
        assert_eq!(status_parts(&create).0, texts::MODE_CREATE);

        let mut import = new_state(FileBrowserMode::ImportFile);
        populate_tmp(&mut import);
        assert_eq!(status_parts(&import).0, texts::MODE_IMPORT);
    }

    #[test]
    fn status_parts_open_file_advertises_search_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("[/]search"),
            "hint should advertise search: {hint}"
        );
    }

    #[test]
    fn test_render_create_file_no_name_input_when_focus_is_list() {
        // In CreateFile mode but focus is on list → no name input area
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::List;
        let text = render_to_text(&mut state);
        // Title should be "Create New Secret File"
        assert!(
            text.contains("Create New Secret File"),
            "title mismatch:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_shows_name_input_when_focused() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let text = render_to_text(&mut state);
        assert!(
            text.contains("Name:"),
            "should show Name: input when focused:\n{text}"
        );
    }

    #[test]
    fn status_parts_name_input_shows_confirm_and_cancel() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let (mode, _, hint) = status_parts(&state);
        assert_eq!(mode, texts::MODE_CREATE);
        assert!(
            hint.contains("Enter:confirm") && hint.contains("Esc:cancel"),
            "the name input should route confirm/cancel to the status bar: {hint}"
        );
    }

    #[test]
    fn test_render_create_file_name_input_is_popup() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let text = render_to_text(&mut state);
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
        let text = render_to_text(&mut state);
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
        let text = render_to_text(&mut state);
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
        let text = render_to_text(&mut state);
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
        let text = render_to_text(&mut state);
        assert!(
            text.contains("myvault"),
            "should show the typed name:\n{text}"
        );
    }

    #[test]
    fn status_parts_search_active_shows_query() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.search_active = true;
        state.search_query = "test".to_string();
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("Search: test"),
            "the search prompt should be the hint: {hint}"
        );
    }

    #[test]
    fn test_render_error_bar_shows_message() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.error = Some("Permission denied".to_string());
        let text = render_to_text(&mut state);
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
        let text = render_to_text(&mut state);
        // Searching for something non-existent
        assert!(
            text.contains("No matching files") || text.contains("(empty directory)"),
            "should show empty state message:\n{text}"
        );
        let _ = std::fs::remove_dir(empty_dir);
    }

    #[test]
    fn status_parts_create_file_advertises_new_hint() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("[n]new"),
            "CreateFile should advertise the new file/folder hint: {hint}"
        );
    }

    #[test]
    fn status_parts_open_file_hides_new_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let (_, _, hint) = status_parts(&state);
        assert!(
            !hint.contains("[n]new"),
            "OpenFile must not advertise the new file/folder hint: {hint}"
        );
    }

    #[test]
    fn status_parts_pending_prefix_shows_new_file_prompt() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.pending = NewFilePrefix::Pending;
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("n:new file") && hint.contains("N:new folder"),
            "the pending n-prefix prompt should be the hint: {hint}"
        );
    }

    #[test]
    fn test_render_dirs_marked_with_arrow_icon() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&mut state);
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

        let text = render_to_text(&mut state);

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

    // ── Status bar: disks + hidden hints ──

    #[test]
    fn status_parts_open_file_advertises_disks_and_hidden() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);

        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("[D]disks"),
            "hint should advertise the locations overlay: {hint}"
        );
        assert!(
            hint.contains("[Ctrl+h]hidden"),
            "hint should advertise the hidden toggle: {hint}"
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
