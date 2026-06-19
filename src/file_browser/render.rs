use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use super::state::{FileBrowserMode, FileBrowserState, FileFilter, FocusTarget};

const DEFAULT_HEIGHT: u16 = 18;

/// Renders the file browser modal over the given screen area.
pub fn render_file_browser(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let overlay_area = make_overlay(area);

    let title = match state.mode {
        FileBrowserMode::OpenFile => " Open File ",
        FileBrowserMode::CreateFile => " Create New Secret File ",
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);

    // Split the inner area: path bar, list, optional input, help bar
    let mut constraints = vec![
        Constraint::Length(1), // path bar
        Constraint::Min(1),    // file list
        Constraint::Length(2), // options bar (filter + search)
    ];

    let has_error = state.error.is_some();
    let has_file_input = state.mode == FileBrowserMode::CreateFile && state.focus == FocusTarget::Input;

    if has_file_input {
        constraints.insert(3, Constraint::Length(1)); // file name input
    }
    if has_error {
        constraints.insert(4, Constraint::Length(1)); // error bar
    }

    let layout = Layout::vertical(constraints).split(inner);

    render_path_bar(frame, layout[0], state);
    render_file_list(frame, layout[1], state);
    render_options_bar(frame, layout[2], state);

    if has_file_input {
        render_name_input(frame, layout[3], state);
    }
    if has_error {
        render_error_bar(frame, if has_file_input { layout[4] } else { layout[3] }, state);
    }

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
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

    spans.push(Span::styled(
        " [n]new dir ",
        Style::default().fg(Color::DarkGray),
    ));

    if state.mode == FileBrowserMode::CreateFile {
        if state.focus == FocusTarget::Input {
            spans.push(Span::styled(
                " [Tab]list ",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            spans.push(Span::styled(
                " [Tab]name ",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    frame.render_widget(Paragraph::new(Text::from(Line::from(spans))), area);
}

fn render_name_input(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    let name = &state.creating_name;
    let cursor = state.creating_cursor;

    let before = &name[..cursor.min(name.len())];
    let at = name
        .get(cursor..)
        .and_then(|s| s.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_default();
    let after = &name[cursor + at.len()..];

    let cursor_char = if at.is_empty() { " " } else { &at };

    let line = Line::from(vec![
        Span::styled(" Name: ", Style::default().fg(Color::Cyan)),
        Span::raw(before.to_string()),
        Span::styled(
            cursor_char.to_string(),
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after.to_string()),
        Span::raw("  "),
        Span::styled("Enter:confirm", Style::default().fg(Color::DarkGray)),
    ]);

    frame.render_widget(Paragraph::new(Text::from(vec![line])), area);
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
