use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{backend::Backend, Frame, Terminal};

const MENU_ITEMS: &[&str] = &["Open secret file", "Create new secret file"];

pub struct SettingsState {
    selected: usize,
    pub recent_files: Vec<String>,
    pub quit: bool,
}

impl SettingsState {
    pub fn new() -> Self {
        Self {
            selected: 0,
            recent_files: vec![
                "vault_2024.kdbx".to_string(),
                "work_passwords.kdbx".to_string(),
                "personal.kdbx".to_string(),
            ],
            quit: false,
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

            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => state.quit = true,
                KeyCode::Esc => state.quit = true,
                KeyCode::Char('j') | KeyCode::Down => state.select_next(),
                KeyCode::Char('k') | KeyCode::Up => state.select_prev(),
                KeyCode::Enter if state.selected == 0 => {
                    let root = crate::build_demo_tree();
                    let mut tree_state = crate::tree::widget::TreeState::new(root);
                    crate::tree::app_tree::run_app(terminal, &mut tree_state)?;
                }
                _ => {}
            }
        }
    }
}

fn render(frame: &mut Frame, area: Rect, state: &SettingsState) {
    let block = Block::default()
        .title(" Password Keeper ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

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

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "j/k:navigate  Enter:select  q:quit",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}
