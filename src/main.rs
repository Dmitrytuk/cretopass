mod tree;

use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use ratatui::{backend::CrosstermBackend, Terminal};
use tree::node::Node;
use tree::widget::{InputMode, TreeState, TreeWidget};

fn build_demo_tree() -> Node {
    Node::with_children(
        "Knowledge Base",
        "",
        vec![
            Node::with_children(
                "Rust",
                "Systems language focused on safety and performance",
                vec![
                    Node::new("Ownership", "Borrow checker, lifetimes, and memory management"),
                    Node::new("Traits", "Shared behavior via trait definitions and implementations"),
                    Node::new("Async", "Async/await, futures, and tokio runtime"),
                ],
            ),
            Node::with_children(
                "Python",
                "High-level general purpose programming language",
                vec![
                    Node::new("AsyncIO", "Event loop, coroutines, and async/await syntax"),
                    Node::new("Typing", "Type hints, MyPy, and gradual typing"),
                    Node::new("Packaging", "pip, venv, poetry, and dependency management"),
                ],
            ),
            Node::with_children(
                "Docker",
                "Container platform for developing and running applications",
                vec![
                    Node::new("Images", "Building custom images with Dockerfiles"),
                    Node::new("Compose", "Multi-container applications with docker-compose.yml"),
                    Node::new("Volumes", "Persistent data storage and sharing"),
                ],
            ),
        ],
    )
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let root = build_demo_tree();
    let mut state = TreeState::new(root);

    let res = run_app(&mut terminal, &mut state);

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
) -> io::Result<()> {
    loop {
        if state.quit {
            break Ok(());
        }

        terminal.draw(|f| ui(f, state))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match &state.mode {
                InputMode::Normal => handle_normal(key, state)?,
                InputMode::Search => handle_search(key, state)?,
                InputMode::AwaitingDd => {
                    state.mode = InputMode::Normal;
                    if key.code == KeyCode::Char('d') {
                        state.cut_selected();
                    }
                }
                InputMode::Adding {..} => handle_adding(key, state)?,
                InputMode::Editing {..} => handle_editing(key, state)?,
                InputMode::Detail => handle_detail(key, state)?,
            }
        }
    }
}

fn is_leaf_selected(state: &TreeState) -> bool {
    state
        .visible
        .get(state.selected)
        .map(|v| !v.is_parent)
        .unwrap_or(false)
}

fn handle_normal(key: event::KeyEvent, state: &mut TreeState) -> io::Result<()> {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        state.quit = true;
        return Ok(());
    }
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => {
            state.quit = true;
        }
        KeyCode::Char('j') | KeyCode::Down => {
            state.select_next();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            state.select_prev();
        }
        KeyCode::Char('l') | KeyCode::Right => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail;
            } else {
                state.expand_collapse(true);
            }
        }
        KeyCode::Char('h') | KeyCode::Left => {
            state.expand_collapse(false);
        }
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.mode = InputMode::Detail;
        }
        KeyCode::Enter => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail;
            } else {
                state.toggle_expand();
            }
        }
        KeyCode::Char(' ') => {
            state.toggle_expand();
        }
        KeyCode::Char('d') => {
            state.mode = InputMode::AwaitingDd;
        }
        KeyCode::Char('p') => {
            state.paste_sibling();
        }
        KeyCode::Char('P') => {
            state.paste_child();
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            state.mode = InputMode::Detail;
        }
        KeyCode::Char('o') | KeyCode::Char('O') => {
            state.mode = InputMode::Adding {
                name: String::new(),
                desc: String::new(),
                field: 0,
            };
        }
        KeyCode::Char('/') => {
            state.mode = InputMode::Search;
        }
        KeyCode::Esc => {
            state.clear_search();
        }
        _ => {}
    }
    Ok(())
}

fn handle_search(key: event::KeyEvent, state: &mut TreeState) -> io::Result<()> {
    match key.code {
        KeyCode::Enter => {
            state.mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            state.clear_search();
            state.mode = InputMode::Normal;
        }
        KeyCode::Backspace => {
            let mut q = state.search_query.clone();
            q.pop();
            state.set_search(q);
        }
        KeyCode::Char(c) => {
            let mut q = state.search_query.clone();
            q.push(c);
            state.set_search(q);
        }
        _ => {}
    }
    Ok(())
}

fn handle_adding(key: event::KeyEvent, state: &mut TreeState) -> io::Result<()> {
    let (name, desc, field) = match &state.mode {
        InputMode::Adding { name, desc, field } => (name.clone(), desc.clone(), *field),
        _ => return Ok(()),
    };

    match key.code {
        KeyCode::Enter => {
            state.add_child_to_selected(name, desc);
            state.mode = InputMode::Normal;
        }
        KeyCode::Esc => {
            state.mode = InputMode::Normal;
        }
        KeyCode::Tab | KeyCode::Down => {
            let new_field = (field + 1) % 2;
            state.mode = InputMode::Adding { name, desc, field: new_field };
        }
        KeyCode::Backspace => {
            let (new_name, new_desc) = if field == 0 {
                let mut n = name;
                n.pop();
                (n, desc)
            } else {
                let mut d = desc;
                d.pop();
                (name, d)
            };
            state.mode = InputMode::Adding { name: new_name, desc: new_desc, field };
        }
        KeyCode::Char(c) => {
            let (new_name, new_desc) = if field == 0 {
                let mut n = name;
                n.push(c);
                (n, desc)
            } else {
                let mut d = desc;
                d.push(c);
                (name, d)
            };
            state.mode = InputMode::Adding { name: new_name, desc: new_desc, field };
        }
        _ => {}
    }
    Ok(())
}

fn copy_to_clipboard(text: &str) {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[&["wl-copy"], &["xclip", "-selection", "clipboard"], &["xsel", "-b"]]
    } else {
        return;
    };
    for args in cmds {
        if let Ok(mut child) = std::process::Command::new(args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            break;
        }
    }
}

fn handle_editing(key: event::KeyEvent, state: &mut TreeState) -> io::Result<()> {
    let (node_id, name, desc, field, cursor) = match &state.mode {
        InputMode::Editing { node_id, name, desc, field, cursor } => {
            (*node_id, name.clone(), desc.clone(), *field, *cursor)
        }
        _ => return Ok(()),
    };

    let active_len = if field == 0 { name.len() } else { desc.len() };

    match key.code {
        KeyCode::Enter => {
            state.update_node(node_id, name, desc);
            state.mode = InputMode::Detail;
        }
        KeyCode::Esc => {
            state.mode = InputMode::Detail;
        }
        KeyCode::Tab | KeyCode::Down => {
            let new_field = (field + 1) % 2;
            let new_cursor = if new_field == 0 { name.len() } else { desc.len() };
            state.mode = InputMode::Editing { node_id, name, desc, field: new_field, cursor: new_cursor };
        }
        KeyCode::Up => {
            let new_field = if field == 0 { 1 } else { 0 };
            let new_cursor = if new_field == 0 { name.len() } else { desc.len() };
            state.mode = InputMode::Editing { node_id, name, desc, field: new_field, cursor: new_cursor };
        }
        KeyCode::Left => {
            let new_cursor = cursor.saturating_sub(1);
            state.mode = InputMode::Editing { node_id, name, desc, field, cursor: new_cursor };
        }
        KeyCode::Right => {
            let new_cursor = (cursor + 1).min(active_len);
            state.mode = InputMode::Editing { node_id, name, desc, field, cursor: new_cursor };
        }
        KeyCode::Home => {
            state.mode = InputMode::Editing { node_id, name, desc, field, cursor: 0 };
        }
        KeyCode::End => {
            state.mode = InputMode::Editing { node_id, name, desc, field, cursor: active_len };
        }
        KeyCode::Backspace => {
            if cursor == 0 {
                state.mode = InputMode::Editing { node_id, name, desc, field, cursor };
                return Ok(());
            }
            let (new_name, new_desc, new_cursor) = if field == 0 {
                let mut n = name;
                n.remove(cursor - 1);
                (n, desc, cursor - 1)
            } else {
                let mut d = desc;
                d.remove(cursor - 1);
                (name, d, cursor - 1)
            };
            state.mode = InputMode::Editing { node_id, name: new_name, desc: new_desc, field, cursor: new_cursor };
        }
        KeyCode::Char(c) => {
            let (new_name, new_desc, new_cursor) = if field == 0 {
                let mut n = name;
                n.insert(cursor, c);
                (n, desc, cursor + 1)
            } else {
                let mut d = desc;
                d.insert(cursor, c);
                (name, d, cursor + 1)
            };
            state.mode = InputMode::Editing { node_id, name: new_name, desc: new_desc, field, cursor: new_cursor };
        }
        _ => {}
    }
    Ok(())
}

fn handle_detail(key: event::KeyEvent, state: &mut TreeState) -> io::Result<()> {
    match key.code {
        KeyCode::Esc | KeyCode::Enter | KeyCode::Char(' ') => {
            state.mode = InputMode::Normal;
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            if let Some(node) = state.current_node() {
                let cursor = node.name.len();
                state.mode = InputMode::Editing {
                    node_id: node.id,
                    name: node.name,
                    desc: node.description,
                    field: 0,
                    cursor,
                };
            }
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            if let Some(node) = state.current_node() {
                let text = format!("{}\n---\n{}", node.name, node.description);
                copy_to_clipboard(&text);
            }
        }
        _ => {}
    }
    Ok(())
}

fn ui(frame: &mut Frame, state: &mut TreeState) {
    let area = frame.area();

    let layout = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    TreeWidget::render(frame, layout[0], state);
    render_status_bar(frame, layout[1], state);
    render_help_bar(frame, layout[2], state);

    match &state.mode {
        InputMode::Search => render_search_overlay(frame, area, state),
        InputMode::Adding {..} => render_add_overlay(frame, area, state),
        InputMode::Editing {..} => render_edit_overlay(frame, area, state),
        InputMode::Detail => render_detail_overlay(frame, area, state),
        _ => {}
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (mode_str, mode_style) = match &state.mode {
        InputMode::Normal => ("NORMAL", Style::default().bg(Color::DarkGray).fg(Color::White)),
        InputMode::Search => ("SEARCH", Style::default().bg(Color::Blue).fg(Color::White)),
        InputMode::AwaitingDd => ("AWAIT DD", Style::default().bg(Color::Red).fg(Color::White)),
        InputMode::Detail => ("DETAIL", Style::default().bg(Color::Magenta).fg(Color::White)),
        InputMode::Editing {..} => ("EDIT", Style::default().bg(Color::Yellow).fg(Color::Black)),
        InputMode::Adding {..} => ("ADD", Style::default().bg(Color::Green).fg(Color::Black)),
    };

    let name = state.current_name().unwrap_or_default();

    let text = Text::from(Line::from(vec![
        Span::styled(format!(" {} ", mode_str), mode_style.add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(
            format!(
                "{} items{} | {}",
                state.num_visible(),
                if state.search_active {
                    format!(" ({} matches)", state.num_matched())
                } else {
                    String::new()
                },
                if state.search_active {
                    format!("/{}", state.search_query)
                } else {
                    name
                }
            ),
            Style::default().fg(Color::Gray),
        ),
    ]));

    frame.render_widget(Paragraph::new(text), area);
}

fn render_help_bar(frame: &mut Frame, area: Rect, state: &TreeState) {
    let help = match &state.mode {
        InputMode::Normal => {
            "j/k:nav  h/l:collapse  Enter/l:open  i:detail  dd:cut  p:paste  P:paste_child  o:add  /:search  q:quit"
        }
        InputMode::Search => "Type to search  Enter:done  Esc:cancel",
        InputMode::AwaitingDd => "Press d again to cut",
        InputMode::Detail => "i:edit  c:copy  Esc/Enter:close",
        InputMode::Editing {..} => "Tab:switch field  Enter:save  Esc:cancel",
        InputMode::Adding {..} => "Tab:switch field  Enter:save  Esc:cancel",
    };

    let text = Text::from(Line::from(vec![Span::styled(
        help,
        Style::default().fg(Color::DarkGray),
    )]));

    frame.render_widget(Paragraph::new(text), area);
}

fn render_search_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 1,
        width: area.width / 2,
        height: 3,
    };

    let block = Block::default()
        .title(" Search ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let text = Text::from(Line::from(vec![Span::styled(
        format!("/{}", state.search_query),
        Style::default().fg(Color::White),
    )]));

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(text), inner);
}

fn render_add_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (name, desc, field) = match &state.mode {
        InputMode::Adding { name, desc, field } => (name.clone(), desc.clone(), *field),
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 5,
        y: area.height / 2 - 3,
        width: area.width * 3 / 5,
        height: 7,
    };

    let block = Block::default()
        .title(" Add Node ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let name_style = if field == 0 {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };

    let desc_style = if field == 1 {
        Style::default().bg(Color::DarkGray).fg(Color::White)
    } else {
        Style::default().fg(Color::White)
    };

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(Span::styled(format!("Name: {}", name), name_style)),
        Line::from(Span::styled(format!("Desc: {}", desc), desc_style)),
        Line::from(""),
        Line::from(Span::styled(
            "Tab:switch  Enter:save  Esc:cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn cursor_field_spans(label: &str, text: &str, cursor: usize, is_active: bool) -> Vec<Span<'static>> {
    let label_style = Style::default().fg(Color::DarkGray);
    let dim_style = Style::default().fg(Color::DarkGray);
    let normal_style = Style::default().fg(Color::White);
    let inverted_style = Style::default().bg(Color::White).fg(Color::Black);

    if !is_active {
        return vec![
            Span::styled(label.to_string(), label_style),
            Span::styled(text.to_string(), dim_style),
        ];
    }

    let pos = cursor.min(text.len());
    let (before, rest) = text.split_at(pos);
    let ch = rest.chars().next();
    let after = &rest[ch.map(|c| c.len_utf8()).unwrap_or(0)..];

    let mut spans = vec![Span::styled(label.to_string(), label_style)];
    if !before.is_empty() {
        spans.push(Span::styled(before.to_string(), normal_style));
    }
    match ch {
        Some(c) => {
            let mut s = String::new();
            s.push(c);
            spans.push(Span::styled(s, inverted_style));
        }
        None => {
            spans.push(Span::styled(" ".to_string(), inverted_style));
        }
    }
    if !after.is_empty() {
        spans.push(Span::styled(after.to_string(), normal_style));
    }
    spans
}

fn render_edit_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (name, desc, field, cursor) = match &state.mode {
        InputMode::Editing { name, desc, field, cursor, .. } => (name.clone(), desc.clone(), *field, *cursor),
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 5,
        y: area.height / 2 - 3,
        width: area.width * 3 / 5,
        height: 7,
    };

    let block = Block::default()
        .title(" Edit Node ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(cursor_field_spans("Name: ", &name, cursor, field == 0)),
        Line::from(cursor_field_spans("Desc: ", &desc, cursor, field == 1)),
        Line::from(""),
        Line::from(Span::styled(
            "Tab:switch  Enter:save  Esc:cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_detail_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let node = match state.current_node() {
        Some(n) => n,
        None => return,
    };

    let desc = if node.description.is_empty() {
        "No description.".to_string()
    } else {
        node.description.clone()
    };

    let overlay_area = Rect {
        x: area.width / 6,
        y: area.height / 3,
        width: area.width * 2 / 3,
        height: 7,
    };

    let block = Block::default()
        .title(format!(" {} ", node.name))
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(desc, Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled(
            "Esc/Enter to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}
