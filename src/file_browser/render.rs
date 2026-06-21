use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::cursor::cursor_field_spans;
use super::state::{FileBrowserMode, FileBrowserState, FileFilter, FocusTarget};

const DEFAULT_HEIGHT: u16 = 18;

/// Renders the file browser modal over the given screen area.
pub fn render_file_browser(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let overlay_area = make_overlay(area);

    // Clear overlay area first, then draw block borders on top
    frame.render_widget(Clear, overlay_area);

    let title = match state.mode {
        FileBrowserMode::OpenFile => " Open File ",
        FileBrowserMode::CreateFile => " Create New Secret File ",
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    frame.render_widget(&block, overlay_area);

    let inner = block.inner(overlay_area);

    // Split the inner area: path bar, list, optional inputs/confirm, help bar
    let mut constraints = vec![
        Constraint::Length(1), // path bar
        Constraint::Min(1),    // file list
        Constraint::Length(2), // options bar (filter + search + actions)
    ];

    let has_error = state.error.is_some();
    let has_file_input =
        state.mode == FileBrowserMode::CreateFile && state.focus == FocusTarget::Input;
    let has_rename = state.focus == FocusTarget::Rename;
    let has_delete_confirm = state.confirm_delete.is_some();

    if has_file_input {
        constraints.push(Constraint::Length(1)); // file name input
    }
    if has_rename {
        constraints.push(Constraint::Length(1)); // rename input
    }
    if has_delete_confirm {
        constraints.push(Constraint::Length(1)); // delete confirmation
    }
    if has_error {
        constraints.push(Constraint::Length(1)); // error bar
    }

    let layout = Layout::vertical(constraints).split(inner);

    render_path_bar(frame, layout[0], state);
    render_file_list(frame, layout[1], state);
    render_options_bar(frame, layout[2], state);

    let mut next = 3;
    if has_file_input {
        render_file_input(
            frame,
            layout[next],
            " Name: ",
            &state.creating_name,
            state.creating_cursor,
            &["Enter:confirm"],
        );
        next += 1;
    }
    if has_rename {
        render_file_input(
            frame,
            layout[next],
            " Rename: ",
            &state.rename_name,
            state.rename_cursor,
            &["Enter:confirm", "Esc:cancel"],
        );
        next += 1;
    }
    if has_delete_confirm {
        render_delete_confirmation(frame, layout[next], state);
        next += 1;
    }
    if has_error {
        render_error_bar(frame, layout[next], state);
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
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
            .bg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(text), area);
}

fn render_file_list(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let indices = state.filtered_indices();
    let list_height = area.height as usize;

    // Determine scroll position
    let sel_pos = indices
        .iter()
        .position(|&i| i == state.selected)
        .unwrap_or(0);

    let scroll = if sel_pos >= list_height.saturating_sub(1) {
        sel_pos.saturating_sub(list_height.saturating_sub(1))
    } else {
        0
    };
    // Sync browser state scroll
    let scroll = scroll.min(indices.len().saturating_sub(1));

    let visible: Vec<Line> = indices
        .iter()
        .skip(scroll)
        .take(list_height)
        .map(|&idx| {
            let entry = &state.entries[idx];
            let is_selected = idx == state.selected;
            let prefix = if is_selected { "▸ " } else { "  " };

            let name_style = if entry.is_dir {
                Style::default().fg(Color::Blue)
            } else if entry.is_age_file {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            let name_style = if is_selected {
                name_style.bg(Color::DarkGray).fg(Color::White)
            } else {
                name_style
            };

            let marker = if entry.is_age_file && !entry.is_dir {
                "[PWD] "
            } else {
                ""
            };

            let marker_style = if is_selected {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Cyan)
            };

            Line::from(vec![
                Span::styled(prefix, Style::default().fg(Color::White)),
                Span::styled(marker, marker_style),
                Span::styled(&entry.name, name_style),
            ])
        })
        .collect();

    if visible.is_empty() {
        let empty_msg = if state.search_active || !state.search_query.is_empty() {
            "  No matching files"
        } else {
            "  (empty directory)"
        };
        let empty = vec![Line::from(Span::styled(
            empty_msg,
            Style::default().fg(Color::DarkGray),
        ))];
        frame.render_widget(Paragraph::new(Text::from(empty)), area);
    } else {
        frame.render_widget(Paragraph::new(Text::from(visible)), area);
    }
}

fn render_options_bar(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let filter_text = match state.filter {
        FileFilter::All => "All",
        FileFilter::PwdKeeperOnly => "PwdKeeper",
    };

    let mut spans = vec![
        Span::styled(
            format!(" Filter: {} ", filter_text),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(" [f] ", Style::default().fg(Color::DarkGray)),
    ];

    if state.search_active {
        spans.push(Span::styled(
            format!(" Search: {}█", state.search_query),
            Style::default().fg(Color::Yellow),
        ));
    } else {
        spans.push(Span::styled(
            " [/]search ",
            Style::default().fg(Color::DarkGray),
        ));
    }

    if state.mode == FileBrowserMode::CreateFile {
        spans.push(Span::styled(
            " [n]new dir ",
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Show rename/delete hints when a PWD-marked file is selected
    if let Some(entry) = state.entries.get(state.selected) {
        if entry.is_age_file && !entry.is_dir {
            spans.push(Span::styled(
                " [r]rename ",
                Style::default().fg(Color::DarkGray),
            ));
            spans.push(Span::styled(
                " [d]delete ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    if state.mode == FileBrowserMode::CreateFile {
        if state.focus == FocusTarget::Input {
            spans.push(Span::styled(
                " [N]list ",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::styled(
                " [N]name ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Text::from(Line::from(spans))), area);
}

fn render_file_input(frame: &mut Frame, area: Rect, label: &str, text: &str, cursor: usize, hints: &[&str]) {
    let mut spans = cursor_field_spans(label, text, cursor, true);
    for hint in hints {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(*hint, Style::default().fg(Color::DarkGray)));
    }
    frame.render_widget(Paragraph::new(Text::from(vec![Line::from(spans)])), area);
}

fn render_delete_confirmation(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    if let Some(ref path_str) = state.confirm_delete {
        let filename = std::path::Path::new(path_str)
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or(std::borrow::Cow::Borrowed(path_str));

        let text = Line::from(vec![
            Span::styled(
                format!(" Delete {}? ", filename),
                Style::default()
                    .bg(Color::Red)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " (y/N) ",
                Style::default().bg(Color::Red).fg(Color::DarkGray),
            ),
        ]);

        frame.render_widget(Paragraph::new(text), area);
    }
}

fn render_error_bar(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    if let Some(ref err) = state.error {
        let text = Line::from(Span::styled(
            format!(" {}", err),
            Style::default().bg(Color::Red).fg(Color::White),
        ));
        frame.render_widget(Paragraph::new(text), area);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::file_browser::fs;

    /// Helper to create a terminal for testing.
    fn make_terminal() -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(80, 24)).unwrap()
    }

    /// Helper to populate state with entries from /tmp (known to exist).
    fn populate_tmp(state: &mut FileBrowserState) {
        state.current_dir = std::path::PathBuf::from("/tmp");
        state.entries = fs::list_dir(std::path::Path::new("/tmp")).unwrap();
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
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            text.contains("/tmp"),
            "path bar should show '/tmp', got:\n{text}"
        );
    }

    #[test]
    fn test_render_open_file_shows_entries() {
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        // At least one entry name should appear in the output
        let has_entry = state.entries.iter().any(|e| text.contains(&e.name));
        assert!(has_entry, "no entry name found in rendered output:\n{text}");
    }

    #[test]
    fn test_render_open_file_shows_filter_hint() {
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
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
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
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
        let mut state = FileBrowserState::new(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::List;
        let text = render_to_text(&state);
        // Title should be "Create New Secret File"
        assert!(
            text.contains("Create New Secret File"),
            "title mismatch:\n{text}"
        );
        // N hint should mention switching to name input
        assert!(
            text.contains("[N]name"),
            "should show N hint to switch to name input:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_shows_name_input_when_focused() {
        let mut state = FileBrowserState::new(FileBrowserMode::CreateFile);
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
    fn test_render_create_file_with_partial_name() {
        let mut state = FileBrowserState::new(FileBrowserMode::CreateFile);
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
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
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
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
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
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
        // Use an empty directory if possible, or filter with PwdKeeperOnly
        // and search for something that won't match
        state.current_dir = std::path::PathBuf::from("/tmp");
        // Create a known empty directory
        let empty_dir = std::path::Path::new("/tmp/__test_empty_xyz__");
        let _ = std::fs::create_dir_all(empty_dir);
        state.entries =
            fs::list_dir(empty_dir).unwrap();
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
    fn test_render_new_dir_hint_shown_in_create_mode() {
        let mut state = FileBrowserState::new(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            text.contains("[n]new dir"),
            "should show new directory hint in CreateFile mode:\n{text}"
        );
    }

    #[test]
    fn test_render_new_dir_hint_hidden_in_open_mode() {
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&state);
        assert!(
            !text.contains("[n]new dir"),
            "should NOT show new directory hint in OpenFile mode:\n{text}"
        );
    }

    #[test]
    fn test_render_dirs_marked_with_arrow_icon() {
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
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

        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile);
        state.current_dir = dir.to_path_buf();
        state.entries = fs::list_dir(dir).unwrap();
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
        assert!(text.contains("myvault.age"), "should show age filename:\n{text}");
        assert!(text.contains("notes.txt"), "should show regular filename:\n{text}");

        let _ = std::fs::remove_dir_all(dir);
    }
}
