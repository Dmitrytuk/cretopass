use std::fs;
use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{backend::Backend, Frame, Terminal};

use crate::file_browser::{self, FileBrowserAction, FileBrowserMode, FileBrowserState};
use crate::settings::active_file::ActiveFileState;
use crate::settings::utils::{self, AppSettings};

const MENU_ITEMS: &[&str] = &[
    "Open secret file",
    "Create new secret file",
    "Settings",
    "Change file password",
];

enum SettingsModal {
    FileBrowser(FileBrowserState),
    PassphraseInput {
        file_path: String,
        file_data: Option<Vec<u8>>,
        passphrase: String,
        cursor: usize,
        error: Option<String>,
    },
    AppSettings {
        setting_idx: usize,
    },
    Alert {
        message: String,
    },
    DeleteConfirm {
        path: String,
    },
    ChangePwConfirm,
    ChangePwInput {
        current: String,
        current_cursor: usize,
        new_pass: String,
        new_cursor: usize,
        confirm: String,
        confirm_cursor: usize,
        focused: usize,
        error: Option<String>,
    },
}

pub struct SettingsState {
    selected: usize,
    pub settings: AppSettings,
    pub quit: bool,
    pub active_file: Option<ActiveFileState>,
    modal: Option<SettingsModal>,
}

impl SettingsState {
    pub fn new() -> Self {
        let settings = utils::load_settings();
        let modal = if settings.open_last_opened_file_on_startup {
            settings.last_opened_file.as_ref().and_then(|path| {
                utils::validate_encrypted_file(path).ok().map(|data| {
                    SettingsModal::PassphraseInput {
                        file_path: path.clone(),
                        file_data: Some(data),
                        passphrase: String::new(),
                        cursor: 0,
                        error: None,
                    }
                })
            })
        } else {
            None
        };
        Self {
            selected: 0,
            settings,
            quit: false,
            active_file: None,
            modal,
        }
    }

    fn total_navigable(&self) -> usize {
        MENU_ITEMS.len() + self.settings.recent_files.len()
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

            let is_file_browser = state
                .modal
                .as_ref()
                .is_some_and(|m| matches!(m, SettingsModal::FileBrowser(_)));

            if state.modal.is_none() {
                handle_settings_key(key, &mut state, terminal)?;
            } else if is_file_browser {
                handle_file_browser_key(key, &mut state, terminal)?;
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
        KeyCode::Char('H') => {
            if let Some(ref active_file) = state.active_file {
                if let Some(ref node) = active_file.node {
                    let mut tree_state = crate::tree::widget::TreeState::new(node.clone());
                    tree_state.active_file_path = active_file.path.clone();
                    tree_state.active_file_passphrase = active_file.passphrase.clone();
                    tree_state.display_full_path = state.settings.display_full_path;
                    crate::tree::app_tree::run_app(terminal, &mut tree_state)?;
                    if !tree_state.return_to_menu {
                        state.quit = true;
                    }
                }
            }
        }
        KeyCode::Enter => {
            let idx = state.selected;
            if idx == 0 {
                let mut fb = FileBrowserState::new(FileBrowserMode::OpenFile);
                fb.set_active_file_path(
                    state
                        .active_file
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(|p| PathBuf::from(p))),
                );
                state.modal = Some(SettingsModal::FileBrowser(fb));
            } else if idx == 1 {
                let mut fb = FileBrowserState::new(FileBrowserMode::CreateFile);
                fb.set_active_file_path(
                    state
                        .active_file
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(|p| PathBuf::from(p))),
                );
                state.modal = Some(SettingsModal::FileBrowser(fb));
            } else if idx == 2 {
                state.modal = Some(SettingsModal::AppSettings { setting_idx: 0 });
            } else if idx == 3 {
                if state.active_file.is_some() {
                    state.modal = Some(SettingsModal::ChangePwConfirm);
                } else {
                    state.modal = Some(SettingsModal::Alert {
                        message: "No file is currently open".to_string(),
                    });
                }
            } else {
                let file_idx = idx - MENU_ITEMS.len();
                if file_idx < state.settings.recent_files.len() {
                    let file_path = state.settings.recent_files[file_idx].clone();

                    if let Some(ref active_file) = state.active_file {
                        if active_file.path.as_deref() == Some(&file_path) {
                            if let Some(ref node) = active_file.node {
                                let mut tree_state =
                                    crate::tree::widget::TreeState::new(node.clone());
                                tree_state.active_file_path = Some(file_path);
                                tree_state.active_file_passphrase = active_file.passphrase.clone();
                                tree_state.display_full_path = state.settings.display_full_path;
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
        KeyCode::Char('d') | KeyCode::Delete => {
            let idx = state.selected;
            if idx >= MENU_ITEMS.len() {
                let file_idx = idx - MENU_ITEMS.len();
                if file_idx < state.settings.recent_files.len() {
                    let path = &state.settings.recent_files[file_idx];
                    let is_active =
                        state.active_file.as_ref().and_then(|af| af.path.as_deref()) == Some(path);
                    if is_active {
                        state.modal = Some(SettingsModal::Alert {
                            message: "Cannot delete the currently open file from recent files"
                                .to_string(),
                        });
                    } else {
                        state.modal = Some(SettingsModal::DeleteConfirm { path: path.clone() });
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ── File browser key handling ──

fn handle_file_browser_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut SettingsState,
    _terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let mut modal = match state.modal.take() {
        Some(SettingsModal::FileBrowser(b)) => b,
        _ => return Ok(()),
    };

    let action = file_browser::handle_key(key, &mut modal);

    match action {
        None => {
            state.modal = Some(SettingsModal::FileBrowser(modal));
        }
        Some(FileBrowserAction::Cancelled) => {}
        Some(FileBrowserAction::Renamed { from, to }) => {
            if let Some(ref active_file) = state.active_file {
                if active_file.path.as_deref() == Some(&from) {
                    state.active_file.as_mut().unwrap().path = Some(to.clone());
                }
            }
            if state.settings.last_opened_file.as_deref() == Some(&from) {
                state.settings.last_opened_file = Some(to.clone());
            }
            for path in state.settings.recent_files.iter_mut() {
                if *path == from {
                    *path = to.clone();
                    break;
                }
            }
            let _ = utils::save_settings(&state.settings);
            state.modal = Some(SettingsModal::FileBrowser(modal));
        }
        Some(FileBrowserAction::Selected(path)) => {
            let creating = modal.mode == FileBrowserMode::CreateFile;

            if creating {
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
    }
    Ok(())
}

// ── Modal key handling (Passphrase, Alert, AppSettings) ──

fn handle_modal_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut SettingsState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    // ── AppSettings modal ──
    if state
        .modal
        .as_ref()
        .is_some_and(|m| matches!(m, SettingsModal::AppSettings { .. }))
    {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(SettingsModal::AppSettings {
                    ref mut setting_idx,
                }) = state.modal
                {
                    *setting_idx = (*setting_idx + 1).min(1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(SettingsModal::AppSettings {
                    ref mut setting_idx,
                }) = state.modal
                {
                    *setting_idx = setting_idx.saturating_sub(1);
                }
            }
            KeyCode::Char(' ') => {
                let idx = match state.modal.as_ref() {
                    Some(SettingsModal::AppSettings { setting_idx }) => *setting_idx,
                    _ => unreachable!(),
                };
                if idx == 0 {
                    state.settings.display_full_path = !state.settings.display_full_path;
                } else {
                    state.settings.open_last_opened_file_on_startup =
                        !state.settings.open_last_opened_file_on_startup;
                }
            }
            KeyCode::Enter => {
                let _ = utils::save_settings(&state.settings);
                state.modal = Some(SettingsModal::Alert {
                    message: "Settings saved".to_string(),
                });
            }
            KeyCode::Esc => {
                state.settings = utils::load_settings();
                state.modal = None;
            }
            _ => {}
        }
        return Ok(());
    }

    // ── DeleteConfirm modal ──
    if state
        .modal
        .as_ref()
        .is_some_and(|m| matches!(m, SettingsModal::DeleteConfirm { .. }))
    {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(SettingsModal::DeleteConfirm { path }) = state.modal.take() {
                    state.settings.recent_files.retain(|f| f != &path);
                    if state.settings.last_opened_file.as_deref() == Some(&path) {
                        state.settings.last_opened_file = None;
                    }
                    let _ = utils::save_settings(&state.settings);
                }
            }
            _ => {
                state.modal = None;
            }
        }
        return Ok(());
    }

    // ── ChangePwConfirm modal ──
    if state
        .modal
        .as_ref()
        .is_some_and(|m| matches!(m, SettingsModal::ChangePwConfirm))
    {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                state.modal = Some(SettingsModal::ChangePwInput {
                    current: String::new(),
                    current_cursor: 0,
                    new_pass: String::new(),
                    new_cursor: 0,
                    confirm: String::new(),
                    confirm_cursor: 0,
                    focused: 0,
                    error: None,
                });
            }
            _ => {
                state.modal = None;
            }
        }
        return Ok(());
    }

    // ── ChangePwInput modal ──
    if state
        .modal
        .as_ref()
        .is_some_and(|m| matches!(m, SettingsModal::ChangePwInput { .. }))
    {
        match key.code {
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                if let Some(SettingsModal::ChangePwInput { ref mut focused, .. }) =
                    state.modal
                {
                    *focused = (*focused + 1) % 3;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(SettingsModal::ChangePwInput { ref mut focused, .. }) =
                    state.modal
                {
                    *focused = focused.saturating_sub(1);
                }
            }
            KeyCode::Char(c) => {
                if let Some(SettingsModal::ChangePwInput {
                    ref mut current,
                    ref mut current_cursor,
                    ref mut new_pass,
                    ref mut new_cursor,
                    ref mut confirm,
                    ref mut confirm_cursor,
                    ref mut focused,
                    ref mut error,
                }) = state.modal
                {
                    *error = None;
                    match *focused {
                        0 => {
                            current.insert(*current_cursor, c);
                            *current_cursor += 1;
                        }
                        1 => {
                            new_pass.insert(*new_cursor, c);
                            *new_cursor += 1;
                        }
                        _ => {
                            confirm.insert(*confirm_cursor, c);
                            *confirm_cursor += 1;
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(SettingsModal::ChangePwInput {
                    ref mut current,
                    ref mut current_cursor,
                    ref mut new_pass,
                    ref mut new_cursor,
                    ref mut confirm,
                    ref mut confirm_cursor,
                    ref mut focused,
                    ref mut error,
                }) = state.modal
                {
                    *error = None;
                    match *focused {
                        0 if *current_cursor > 0 => {
                            *current_cursor -= 1;
                            current.remove(*current_cursor);
                        }
                        1 if *new_cursor > 0 => {
                            *new_cursor -= 1;
                            new_pass.remove(*new_cursor);
                        }
                        2 if *confirm_cursor > 0 => {
                            *confirm_cursor -= 1;
                            confirm.remove(*confirm_cursor);
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Left => {
                if let Some(SettingsModal::ChangePwInput {
                    ref mut current_cursor,
                    ref mut new_cursor,
                    ref mut confirm_cursor,
                    ref mut focused,
                    ..
                }) = state.modal
                {
                    match *focused {
                        0 if *current_cursor > 0 => *current_cursor -= 1,
                        1 if *new_cursor > 0 => *new_cursor -= 1,
                        2 if *confirm_cursor > 0 => *confirm_cursor -= 1,
                        _ => {}
                    }
                }
            }
            KeyCode::Right => {
                if let Some(SettingsModal::ChangePwInput {
                    ref mut current,
                    ref mut current_cursor,
                    ref mut new_pass,
                    ref mut new_cursor,
                    ref mut confirm,
                    ref mut confirm_cursor,
                    ref mut focused,
                    ..
                }) = state.modal
                {
                    match *focused {
                        0 if *current_cursor < current.len() => *current_cursor += 1,
                        1 if *new_cursor < new_pass.len() => *new_cursor += 1,
                        2 if *confirm_cursor < confirm.len() => *confirm_cursor += 1,
                        _ => {}
                    }
                }
            }
            KeyCode::Home => {
                if let Some(SettingsModal::ChangePwInput {
                    ref mut current_cursor,
                    ref mut new_cursor,
                    ref mut confirm_cursor,
                    ref mut focused,
                    ..
                }) = state.modal
                {
                    match *focused {
                        0 => *current_cursor = 0,
                        1 => *new_cursor = 0,
                        _ => *confirm_cursor = 0,
                    }
                }
            }
            KeyCode::End => {
                if let Some(SettingsModal::ChangePwInput {
                    ref mut current,
                    ref mut current_cursor,
                    ref mut new_pass,
                    ref mut new_cursor,
                    ref mut confirm,
                    ref mut confirm_cursor,
                    ref mut focused,
                    ..
                }) = state.modal
                {
                    match *focused {
                        0 => *current_cursor = current.len(),
                        1 => *new_cursor = new_pass.len(),
                        _ => *confirm_cursor = confirm.len(),
                    }
                }
            }
            KeyCode::Esc => {
                state.modal = None;
            }
            KeyCode::Enter => {
                let mut modal = match state.modal.take() {
                    Some(SettingsModal::ChangePwInput {
                        current,
                        current_cursor,
                        new_pass,
                        new_cursor,
                        confirm,
                        confirm_cursor,
                        focused,
                        error,
                    }) => SettingsModal::ChangePwInput {
                        current,
                        current_cursor,
                        new_pass,
                        new_cursor,
                        confirm,
                        confirm_cursor,
                        focused,
                        error,
                    },
                    _ => return Ok(()),
                };
                let (_current, new_pass, _confirm, has_error) = match &mut modal {
                    SettingsModal::ChangePwInput {
                        current,
                        new_pass,
                        confirm,
                        error,
                        ..
                    } => {
                        let is_ok = if current.as_bytes() != state.active_file.as_ref().map(|af| af.passphrase.as_slice()).unwrap_or(&[]) {
                            *error = Some("Current password is incorrect".to_string());
                            false
                        } else if new_pass.is_empty() || confirm.is_empty() {
                            *error = Some("New password fields cannot be empty".to_string());
                            false
                        } else if new_pass != confirm {
                            *error = Some("New passwords do not match".to_string());
                            false
                        } else {
                            true
                        };
                        (current.clone(), new_pass.clone(), confirm.clone(), !is_ok)
                    }
                    _ => unreachable!(),
                };
                if has_error {
                    state.modal = Some(modal);
                } else {
                    // Save with new passphrase and reopen
                    if let Some(ref active_file) = state.active_file.clone() {
                        if let (Some(ref path), Some(ref node)) =
                            (&active_file.path, &active_file.node)
                        {
                            let new_pass_bytes = new_pass.into_bytes();
                            match utils::save_tree(node, path, &new_pass_bytes) {
                                Err(e) => {
                                    state.modal = Some(SettingsModal::ChangePwInput {
                                        current: String::new(),
                                        current_cursor: 0,
                                        new_pass: String::new(),
                                        new_cursor: 0,
                                        confirm: String::new(),
                                        confirm_cursor: 0,
                                        focused: 0,
                                        error: Some(format!("Failed to save: {e}")),
                                    });
                                }
                                Ok(()) => {
                                    state.active_file.as_mut().unwrap().passphrase =
                                        new_pass_bytes.clone();
                                    let mut tree_state =
                                        crate::tree::widget::TreeState::new(node.clone());
                                    tree_state.active_file_path = Some(path.clone());
                                    tree_state.active_file_passphrase = new_pass_bytes;
                                    tree_state.display_full_path =
                                        state.settings.display_full_path;
                                    crate::tree::app_tree::run_app(terminal, &mut tree_state)?;
                                    if !tree_state.return_to_menu {
                                        state.quit = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        return Ok(());
    }

    let mut modal = match state.modal.take() {
        Some(m) => m,
        None => return Ok(()),
    };

    match key.code {
        KeyCode::Esc => {}
        KeyCode::Char(c) => match &mut modal {
            SettingsModal::PassphraseInput {
                passphrase,
                cursor,
                error,
                ..
            } => {
                passphrase.insert(*cursor, c);
                *cursor += 1;
                *error = None;
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {}
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ChangePwInput { .. } => unreachable!(),
        },
        KeyCode::Backspace => match &mut modal {
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
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ChangePwInput { .. } => unreachable!(),
        },
        KeyCode::Left => match &mut modal {
            SettingsModal::PassphraseInput { cursor, .. } => {
                *cursor = cursor.saturating_sub(1);
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ChangePwInput { .. } => unreachable!(),
        },
        KeyCode::Right => match &mut modal {
            SettingsModal::PassphraseInput {
                cursor, passphrase, ..
            } => {
                if *cursor < passphrase.len() {
                    *cursor += 1;
                }
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ChangePwInput { .. } => unreachable!(),
        },
        KeyCode::Home => match &mut modal {
            SettingsModal::PassphraseInput { cursor, .. } => {
                *cursor = 0;
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ChangePwInput { .. } => unreachable!(),
        },
        KeyCode::End => match &mut modal {
            SettingsModal::PassphraseInput {
                cursor, passphrase, ..
            } => {
                *cursor = passphrase.len();
                state.modal = Some(modal);
            }
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ChangePwInput { .. } => unreachable!(),
        },
        KeyCode::Enter => match modal {
            SettingsModal::PassphraseInput {
                file_path,
                file_data,
                passphrase,
                ..
            } => match file_data {
                Some(data) => match utils::decrypt_to_node(&data, &passphrase) {
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

                        let _ = utils::push_recent_file(&mut state.settings, &file_path);

                        let mut tree_state = crate::tree::widget::TreeState::new(node);
                        tree_state.active_file_path = Some(file_path);
                        tree_state.active_file_passphrase = pp_bytes;
                        tree_state.display_full_path = state.settings.display_full_path;
                        crate::tree::app_tree::run_app(terminal, &mut tree_state)?;

                        if !tree_state.return_to_menu {
                            state.quit = true;
                        }
                    }
                },
                None => match utils::create_secret_file(&file_path, &passphrase) {
                    Err(e) => {
                        state.modal = Some(SettingsModal::Alert { message: e });
                    }
                    Ok(node) => {
                        let pp_bytes = passphrase.into_bytes();
                        let active_file = ActiveFileState {
                            path: Some(file_path.clone()),
                            passphrase: pp_bytes.clone(),
                            node: Some(node.clone()),
                        };
                        state.active_file = Some(active_file);

                        let _ = utils::push_recent_file(&mut state.settings, &file_path);

                        let mut tree_state = crate::tree::widget::TreeState::new(node);
                        tree_state.active_file_path = Some(file_path);
                        tree_state.active_file_passphrase = pp_bytes;
                        tree_state.display_full_path = state.settings.display_full_path;
                        crate::tree::app_tree::run_app(terminal, &mut tree_state)?;

                        if !tree_state.return_to_menu {
                            state.quit = true;
                        }
                    }
                },
            },
            SettingsModal::Alert { .. } => {}
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ChangePwInput { .. } => unreachable!(),
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
        Some(SettingsModal::FileBrowser(ref browser)) => {
            file_browser::render_file_browser(frame, area, browser);
        }
        Some(SettingsModal::PassphraseInput { .. }) => render_passphrase_modal(frame, area, state),
        Some(SettingsModal::AppSettings { .. }) => render_app_settings_modal(frame, area, state),
        Some(SettingsModal::Alert { message }) => render_alert_modal(frame, area, message),
        Some(SettingsModal::DeleteConfirm { path }) => {
            render_delete_confirm_modal(frame, area, path)
        }
        Some(SettingsModal::ChangePwConfirm) => render_change_pw_confirm_modal(frame, area),
        Some(SettingsModal::ChangePwInput { .. }) => {
            render_change_pw_input_modal(frame, area, state)
        }
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
        .map(|p| {
            if state.settings.display_full_path {
                format!(" Settings — {} ", p)
            } else {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or(std::borrow::Cow::Borrowed(p));
                format!(" Settings — {} ", name)
            }
        })
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
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, item),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "─── Recent Files ───",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    let menu_len = MENU_ITEMS.len();
    for (i, file) in state.settings.recent_files.iter().enumerate() {
        let idx = menu_len + i;
        let is_selected = state.selected == idx;
        let style = if is_selected {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if is_selected { "▸ " } else { "  " };
        let display = if state.settings.display_full_path {
            file.clone()
        } else {
            std::path::Path::new(file)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.clone())
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, display),
            style,
        )));
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

fn render_app_settings_modal(frame: &mut Frame, area: Rect, state: &SettingsState) {
    let setting_idx = match &state.modal {
        Some(SettingsModal::AppSettings { setting_idx }) => *setting_idx,
        _ => 0,
    };

    let overlay_area = make_overlay(area, 10);
    let block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Cyan));

    let inner = block.inner(overlay_area);

    let rows = [
        (
            "Display full path in title bar",
            state.settings.display_full_path,
        ),
        (
            "Start with last file",
            state.settings.open_last_opened_file_on_startup,
        ),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, (label, value)) in rows.iter().enumerate() {
        let is_selected = i == setting_idx;
        let style = if is_selected {
            Style::default().bg(Color::DarkGray).fg(Color::White)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if is_selected { "▸ " } else { "  " };
        let checkbox = if *value { "[X]" } else { "[ ]" };
        lines.push(Line::from(vec![
            Span::styled(format!("{}{} ", prefix, checkbox), style),
            Span::styled(*label, style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Space:toggle  Enter:save  Esc:cancel",
        Style::default().fg(Color::DarkGray),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_delete_confirm_modal(frame: &mut Frame, area: Rect, path: &str) {
    let overlay_area = make_overlay(area, 7);
    let block = Block::default()
        .title(" Confirm Delete ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Red));

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Remove '{}' from recent files?", path),
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y/Enter:confirm  any key:cancel",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_change_pw_confirm_modal(frame: &mut Frame, area: Rect) {
    let overlay_area = make_overlay(area, 7);
    let block = Block::default()
        .title(" Change Password ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Yellow));

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Are you sure you want to change the password?",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y/Enter:yes  any key:no",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_change_pw_input_modal(frame: &mut Frame, area: Rect, state: &SettingsState) {
    let (current, current_cursor, new_pass, new_cursor, confirm, confirm_cursor, focused, error) =
        match &state.modal {
            Some(SettingsModal::ChangePwInput {
                current,
                current_cursor,
                new_pass,
                new_cursor,
                confirm,
                confirm_cursor,
                focused,
                error,
            }) => (
                current.clone(),
                *current_cursor,
                new_pass.clone(),
                *new_cursor,
                confirm.clone(),
                *confirm_cursor,
                *focused,
                error.clone(),
            ),
            _ => return,
        };

    let overlay_area = make_overlay(area, if error.is_some() { 13 } else { 12 });
    let block = Block::default()
        .title(" Change Password ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black).fg(Color::Yellow));

    let inner = block.inner(overlay_area);

    let fields = [
        ("Current password", &current, current_cursor),
        ("New password", &new_pass, new_cursor),
        ("Confirm new password", &confirm, confirm_cursor),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, (label, value, cursor_pos)) in fields.iter().enumerate() {
        let is_focused = i == focused;
        let masked = "*".repeat(value.len());

        if is_focused {
            let pos = *cursor_pos;
            let before = &masked[..pos.min(masked.len())];
            let at = masked
                .get(pos..)
                .and_then(|s| s.chars().next())
                .map(|c| c.to_string())
                .unwrap_or_default();
            let after = &masked[pos + at.len()..];
            let cursor_char = if at.is_empty() { " " } else { &at };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}: ", label),
                    Style::default().fg(Color::White),
                ),
                Span::raw(before.to_string()),
                Span::styled(
                    cursor_char.to_string(),
                    Style::default().bg(Color::White).fg(Color::Black),
                ),
                Span::raw(after.to_string()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}: ", label),
                    Style::default().fg(Color::White),
                ),
                Span::styled(masked, Style::default().fg(Color::White)),
            ]));
        }
    }

    if let Some(ref err_msg) = error {
        lines.push(Line::from(Span::styled(
            format!(" {}", err_msg),
            Style::default().bg(Color::Red).fg(Color::White),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Tab/↑/↓:switch  Enter:apply  Esc:cancel",
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

    let display_path = if state.settings.display_full_path {
        file_path.clone()
    } else {
        std::path::Path::new(&file_path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.clone())
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
        format!("File: {}", display_path),
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
    let text = Text::from(Line::from(vec![Span::styled(
        " SETTINGS ",
        Style::default()
            .bg(Color::Cyan)
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    )]));
    frame.render_widget(Paragraph::new(text), area);
}

fn render_settings_help_bar(frame: &mut Frame, area: Rect) {
    let text = Text::from(Line::from(vec![Span::styled(
        "j/k:navigate  Enter:select  d:<delete path> q:quit",
        Style::default().fg(Color::DarkGray),
    )]));
    frame.render_widget(Paragraph::new(text), area);
}
