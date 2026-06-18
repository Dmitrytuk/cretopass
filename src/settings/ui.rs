use std::fs;
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{backend::Backend, Frame, Terminal};

use crate::settings::active_file::ActiveFileState;
use crate::settings::utils;

const MENU_ITEMS: &[&str] = &["Open secret file", "Create new secret file"];

enum SettingsModal {
    FileInput {
        path: String,
        cursor: usize,
        creating: bool,
    },
    PassphraseInput {
        file_path: String,
        file_data: Option<Vec<u8>>,
        passphrase: String,
        cursor: usize,
        error: Option<String>,
    },
    Alert { message: String },
}

pub struct SettingsState {
    selected: usize,
    pub recent_files: Vec<String>,
    pub quit: bool,
    pub active_file: Option<ActiveFileState>,
    modal: Option<SettingsModal>,
}

impl SettingsState {
    pub fn new() -> Self {
        let recent_files = utils::load_recent_files().unwrap_or_default();
        Self {
            selected: 0,
            recent_files,
            quit: false,
            active_file: None,
            modal: None,
        }
    }

    fn total_navigable(&self) -> usize {
        MENU_ITEMS.len() + self.recent_files.len()
    }

    fn select_next(&mut self) {
        let total = self.total_navigable();
        if total == 0 {
            return;
        }
        self.selected = (self.selected + 1) % total;
    }

    fn select_prev(&mut self) {
        let total = self.total_navigable();
        if total == 0 {
            return;
        }
        self.selected = (self.selected + total - 1) % total;
    }
}

pub fn run<B: Backend>(terminal: &mut Terminal<B>) -> io::Result<()> {
    let mut state = SettingsState::new();
    loop {
        if state.quit {
            break Ok(());
        }

        terminal.draw(|f| {
            let area = f.area();
            render(f, area, &state);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            if state.modal.is_none() {
                handle_settings_key(key, &mut state, terminal)?;
            } else {
                handle_modal_key(key, &mut state, terminal)?;
            }
        }
    }
}

// ── Settings menu key handling (no modal active) ──

fn handle_settings_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut SettingsState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => state.quit = true,
        KeyCode::Esc => state.quit = true,
        KeyCode::Char('j') | KeyCode::Down => state.select_next(),
        KeyCode::Char('k') | KeyCode::Up => state.select_prev(),
        KeyCode::Enter => {
            let idx = state.selected;
            if idx == 0 {
                state.modal = Some(SettingsModal::FileInput {
                    path: String::new(),
                    cursor: 0,
                    creating: false,
                });
            } else if idx == 1 {
                state.modal = Some(SettingsModal::FileInput {
                    path: String::new(),
                    cursor: 0,
                    creating: true,
                });
            } else {
                let file_idx = idx - MENU_ITEMS.len();
                if file_idx < state.recent_files.len() {
                    let file_path = state.recent_files[file_idx].clone();

                    // If this is the currently active file, re-open the tree directly
                    if let Some(ref active_file) = state.active_file {
                        if active_file.path.as_deref() == Some(&file_path) {
                            if let Some(ref node) = active_file.node {
                                let mut tree_state =
                                    crate::tree::widget::TreeState::new(node.clone());
                                tree_state.active_file_path = Some(file_path);
                                tree_state.active_file_passphrase = active_file.passphrase.clone();
                                crate::tree::app_tree::run_app(terminal, &mut tree_state)?;
                                if !tree_state.return_to_menu {
                                    state.quit = true;
                                }
                                return Ok(());
                            }
                        }
                    }

                    match utils::validate_encrypted_file(&file_path) {
                        Err(e) => {
                            state.modal = Some(SettingsModal::Alert { message: e });
                        }
                        Ok(data) => {
                            state.modal = Some(SettingsModal::PassphraseInput {
                                file_path,
                                file_data: Some(data),
                                passphrase: String::new(),
                                cursor: 0,
                                error: None,
                            });
                        }
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ── Modal key handling ──

fn handle_modal_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut SettingsState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let mut modal = match state.modal.take() {
        Some(m) => m,
        None => return Ok(()),
    };

    match key.code {
        KeyCode::Esc => {}
        KeyCode::Char(c) => match &mut modal {
            SettingsModal::FileInput { path, cursor, .. } => {
                path.insert(*cursor, c);
                *cursor += 1;
                state.modal = Some(modal);
            }
            SettingsModal::PassphraseInput {
                passphrase, cursor, error, ..
            } => {
                passphrase.insert(*cursor, c);
                *cursor += 1;
                *error = None;
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {}
        },
        KeyCode::Backspace => match &mut modal {
            SettingsModal::FileInput { path, cursor, .. } => {
                if *cursor > 0 {
                    *cursor -= 1;
                    path.remove(*cursor);
                }
                state.modal = Some(modal);
            }
            SettingsModal::PassphraseInput {
                passphrase, cursor, ..
            } => {
                if *cursor > 0 {
                    *cursor -= 1;
                    passphrase.remove(*cursor);
                }
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
        },
        KeyCode::Left => match &mut modal {
            SettingsModal::FileInput { cursor, .. }
            | SettingsModal::PassphraseInput { cursor, .. } => {
                *cursor = cursor.saturating_sub(1);
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
        },
        KeyCode::Right => {
            match &mut modal {
                SettingsModal::FileInput { cursor, path, .. } => {
                    if *cursor < path.len() {
                        *cursor += 1;
                    }
                }
                SettingsModal::PassphraseInput { cursor, passphrase, .. } => {
                    if *cursor < passphrase.len() {
                        *cursor += 1;
                    }
                }
                SettingsModal::Alert { .. } => {}
            }
            state.modal = Some(modal);
        }
        KeyCode::Home => match &mut modal {
            SettingsModal::FileInput { cursor, .. }
            | SettingsModal::PassphraseInput { cursor, .. } => {
                *cursor = 0;
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
        },
        KeyCode::End => match &mut modal {
            SettingsModal::FileInput { cursor, path, .. } => {
                *cursor = path.len();
                state.modal = Some(modal);
            }
            SettingsModal::PassphraseInput { cursor, passphrase, .. } => {
                *cursor = passphrase.len();
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
        },
        KeyCode::Enter => match modal {
            SettingsModal::FileInput {
                path, creating, ..
            } => {
                if path.is_empty() {
                    if let Some(picked) = utils::open_file_dialog() {
                        state.modal = Some(SettingsModal::FileInput {
                            path: picked,
                            cursor: 0,
                            creating,
                        });
                    }
                } else if creating {
                    // Probe writability by writing and removing a temp file
                    let probe_path = format!("{}.tmp", path);
                    let writable = fs::write(&probe_path, b"").is_ok();
                    let _ = fs::remove_file(&probe_path);

                    if writable {
                        state.modal = Some(SettingsModal::PassphraseInput {
                            file_path: path,
                            file_data: None,
                            passphrase: String::new(),
                            cursor: 0,
                            error: None,
                        });
                    } else {
                        state.modal = Some(SettingsModal::Alert {
                            message: "Cannot write to this path".to_string(),
                        });
                    }
                } else {
                    match utils::validate_encrypted_file(&path) {
                        Err(e) => {
                            state.modal = Some(SettingsModal::Alert { message: e });
                        }
                        Ok(data) => {
                            state.modal = Some(SettingsModal::PassphraseInput {
                                file_path: path,
                                file_data: Some(data),
                                passphrase: String::new(),
                                cursor: 0,
                                error: None,
                            });
                        }
                    }
                }
            }
            SettingsModal::PassphraseInput {
                file_path,
                file_data,
                passphrase,
                ..
            } => {
                match file_data {
                    Some(data) => {
                        match utils::decrypt_to_node(&data, &passphrase) {
                            Err(e) => {
                                state.modal = Some(SettingsModal::PassphraseInput {
                                    file_path,
                                    file_data: Some(data),
                                    passphrase,
                                    cursor: 0,
                                    error: Some(format!("wrong password: {e}")),
                                });
                            }
                            Ok(node) => {
                                let pp_bytes = passphrase.into_bytes();
                                let active_file = ActiveFileState {
                                    path: Some(file_path.clone()),
                                    passphrase: pp_bytes.clone(),
                                    node: Some(node.clone()),
                                };
                                state.active_file = Some(active_file);

                                let mut files = state.recent_files.clone();
                                let _ = utils::push_recent_file(&mut files, &file_path);
                                state.recent_files = files;

                                let mut tree_state =
                                    crate::tree::widget::TreeState::new(node);
                                tree_state.active_file_path = Some(file_path);
                                tree_state.active_file_passphrase = pp_bytes;
                                crate::tree::app_tree::run_app(
                                    terminal,
                                    &mut tree_state,
                                )?;

                                if !tree_state.return_to_menu {
                                    state.quit = true;
                                }
                            }
                        }
                    }
                    None => {
                        match utils::create_secret_file(&file_path, &passphrase) {
                            Err(e) => {
                                state.modal = Some(SettingsModal::Alert {
                                    message: e,
                                });
                            }
                            Ok(node) => {
                                let pp_bytes = passphrase.into_bytes();
                                let active_file = ActiveFileState {
                                    path: Some(file_path.clone()),
                                    passphrase: pp_bytes.clone(),
                                    node: Some(node.clone()),
                                };
                                state.active_file = Some(active_file);

                                let mut files = state.recent_files.clone();
                                let _ =
                                    utils::push_recent_file(&mut files, &file_path);
                                state.recent_files = files;

                                let mut tree_state =
                                    crate::tree::widget::TreeState::new(node);
                                tree_state.active_file_path = Some(file_path);
                                tree_state.active_file_passphrase = pp_bytes;
                                crate::tree::app_tree::run_app(
                                    terminal,
                                    &mut tree_state,
                                )?;

                                if !tree_state.return_to_menu {
                                    state.quit = true;
                                }
                            }
                        }
                    }
                }
            }
            SettingsModal::Alert { .. } => {}
        },
        _ => {
            state.modal = Some(modal);
        }
    }
    Ok(())
}

// ── Rendering ──

fn render(frame: &mut Frame, area: Rect, state: &SettingsState) {
    render_settings_window(frame, area, state);

    match &state.modal {
        Some(SettingsModal::FileInput { .. }) => render_file_input_modal(frame, area, state),
        Some(SettingsModal::PassphraseInput { .. }) => render_passphrase_modal(frame, area, state),
        Some(SettingsModal::Alert { message }) => render_alert_modal(frame, area, message),
        None => {}
    }
}

fn render_settings_window(frame: &mut Frame, area: Rect, state: &SettingsState) {
    let layout = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    let title = state
        .active_file
        .as_ref()
        .and_then(|af| af.path.as_deref())
        .map(|p| format!(" Settings — {} ", p))
        .unwrap_or_else(|| " Settings ".to_string());

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(layout[0]);
    frame.render_widget(block, layout[0]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, item) in MENU_ITEMS.iter().enumerate() {
        let is_selected = state.selected == i;
        let style = if is_selected {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if is_selected { "▸ " } else { "  " };
        lines.push(Line::from(Span::styled(format!("{}{}", prefix, item), style)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "─── Recent Files ───",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let menu_len = MENU_ITEMS.len();
    for (i, file) in state.recent_files.iter().enumerate() {
        let idx = menu_len + i;
        let is_selected = state.selected == idx;
        let style = if is_selected {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if is_selected { "▸ " } else { "  " };
        lines.push(Line::from(Span::styled(format!("{}{}", prefix, file), style)));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);

    render_settings_status_bar(frame, layout[1], state);
    render_settings_help_bar(frame, layout[2]);
}

fn make_overlay(area: Rect, height: u16) -> Rect {
    let overlay_height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 5,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width * 3 / 5,
        height: overlay_height,
    }
}

fn render_file_input_modal(frame: &mut Frame, area: Rect, state: &SettingsState) {
    let (path, cursor, creating) = match &state.modal {
        Some(SettingsModal::FileInput {
            path, cursor, creating, ..
        }) => (path.clone(), *cursor, *creating),
        _ => return,
    };

    let overlay_area = make_overlay(area, 7);

    let title = if creating {
        " Create New Secret File "
    } else {
        " Open File "
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);

    let mut lines = vec![Line::from("")];
    if creating {
        lines.push(Line::from(Span::styled(
            "Enter a path and filename for the new secret file",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            "Enter file path or press Enter on empty field to browse",
            Style::default().fg(Color::DarkGray),
        )));
    }
    lines.push(Line::from(""));

    let before = &path[..cursor.min(path.len())];
    let at = path
        .get(cursor..)
        .and_then(|s| s.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_default();
    let after = &path[cursor + at.len()..];

    let cursor_char = if at.is_empty() { " " } else { &at };

    lines.push(Line::from(vec![
        Span::styled("Path: ", Style::default().fg(Color::Cyan)),
        Span::raw(before.to_string()),
        Span::styled(
            cursor_char.to_string(),
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after.to_string()),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter:confirm  Esc:cancel",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_passphrase_modal(frame: &mut Frame, area: Rect, state: &SettingsState) {
    let (file_path, passphrase, cursor, error) = match &state.modal {
        Some(SettingsModal::PassphraseInput {
            file_path,
            passphrase,
            cursor,
            error,
            ..
        }) => (
            file_path.clone(),
            passphrase.clone(),
            *cursor,
            error.clone(),
        ),
        _ => return,
    };

    let overlay_area = make_overlay(area, if error.is_some() { 9 } else { 8 });

    let block = Block::default()
        .title(" Enter Passphrase ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("File: {}", file_path),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let display = "*".repeat(passphrase.len());
    let before = &display[..cursor.min(display.len())];
    let at = display
        .get(cursor..)
        .and_then(|s| s.chars().next())
        .map(|c| c.to_string())
        .unwrap_or_default();
    let after = &display[cursor + at.len()..];

    let cursor_char = if at.is_empty() { " " } else { &at };

    lines.push(Line::from(vec![
        Span::styled("Passphrase: ", Style::default().fg(Color::Cyan)),
        Span::raw(before.to_string()),
        Span::styled(
            cursor_char.to_string(),
            Style::default().bg(Color::White).fg(Color::Black),
        ),
        Span::raw(after.to_string()),
    ]));

    if let Some(ref err_msg) = error {
        lines.push(Line::from(Span::styled(
            format!(" {}", err_msg),
            Style::default().bg(Color::Red).fg(Color::White),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Enter:confirm  Esc:cancel",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_alert_modal(frame: &mut Frame, area: Rect, message: &str) {
    let overlay_area = make_overlay(area, 6);
    let block = Block::default()
        .title(" Alert ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Red));

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(Span::styled(
            "Press any key to dismiss",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).alignment(Alignment::Center),
        inner,
    );
}

fn render_settings_status_bar(frame: &mut Frame, area: Rect, _state: &SettingsState) {
    let text = Text::from(Line::from(vec![
        Span::styled(
            " SETTINGS ",
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(Paragraph::new(text), area);
}

fn render_settings_help_bar(frame: &mut Frame, area: Rect) {
    let text = Text::from(Line::from(vec![Span::styled(
        "j/k:navigate  Enter:select  q:quit",
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(Paragraph::new(text), area);
}
