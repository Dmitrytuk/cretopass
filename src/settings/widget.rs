use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};

use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{backend::Backend, Frame, Terminal};

use crate::file_browser::{self, FileBrowserAction, FileBrowserMode, FileBrowserState};
use crate::file_handling;
use crate::settings::active_file::ActiveFileState;
use crate::settings::utils::{self, AppSettings};
use crate::theming::Action;
use crate::theming::{texts, ElementStyle, Icon};

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
    ConfirmNoPassphrase {
        file_path: String,
    },
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
    ExportSelectType {
        selected_type: usize,
    },
    ExportConfirm {
        export_type: String,
    },
}

pub struct SettingsState {
    selected: usize,
    pub settings: AppSettings,
    pub quit: bool,
    pub active_file: Option<ActiveFileState>,
    modal: Option<SettingsModal>,
    pub status: Option<&'static str>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsState {
    pub fn new() -> Self {
        let settings = utils::load_settings();
        let modal = if settings.open_last_opened_file_on_startup {
            settings.last_opened_file.as_ref().and_then(|path| {
                file_handling::validate_encrypted_file(path)
                    .ok()
                    .map(|data| SettingsModal::PassphraseInput {
                        file_path: path.clone(),
                        file_data: Some(data),
                        passphrase: String::new(),
                        cursor: 0,
                        error: None,
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
            status: None,
        }
    }

    fn total_navigable(&self) -> usize {
        texts::MENU_ITEMS.len() + self.settings.recent_files.len()
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
    match Action::from_settings_menu(&key) {
        Action::Quit => state.quit = true,
        Action::NavDown => state.select_next(),
        Action::NavUp => state.select_prev(),
        Action::ReturnToMenu => {
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
        Action::Confirm => {
            let idx = state.selected;
            if idx == 0 {
                let mut fb = FileBrowserState::new(FileBrowserMode::OpenFile);
                fb.set_active_file_path(
                    state
                        .active_file
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(PathBuf::from)),
                );
                state.modal = Some(SettingsModal::FileBrowser(fb));
            } else if idx == 1 {
                let mut fb = FileBrowserState::new(FileBrowserMode::CreateFile);
                fb.set_active_file_path(
                    state
                        .active_file
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(PathBuf::from)),
                );
                state.modal = Some(SettingsModal::FileBrowser(fb));
            } else if idx == 2 {
                state.modal = Some(SettingsModal::AppSettings { setting_idx: 0 });
            } else if idx == 3 {
                if state.active_file.is_some() {
                    state.modal = Some(SettingsModal::ChangePwConfirm);
                } else {
                    state.modal = Some(SettingsModal::Alert {
                        message: texts::MSG_NO_OPEN_FILE.to_string(),
                    });
                }
            } else if idx == 4 {
                if state.active_file.is_some() {
                    state.modal = Some(SettingsModal::ExportSelectType { selected_type: 0 });
                } else {
                    state.modal = Some(SettingsModal::Alert {
                        message: texts::MSG_NO_OPEN_FILE.to_string(),
                    });
                }
            } else if idx == 5 {
                let mut fb = FileBrowserState::new(FileBrowserMode::ImportFile);
                fb.set_active_file_path(
                    state
                        .active_file
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(PathBuf::from)),
                );
                state.modal = Some(SettingsModal::FileBrowser(fb));
            } else {
                let file_idx = idx - texts::MENU_ITEMS.len();
                if file_idx < state.settings.recent_files.len() {
                    let file_path: String = state.settings.recent_files[file_idx].clone();

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

                    match file_handling::validate_encrypted_file(&file_path) {
                        Err(e) => {
                            state.modal = Some(SettingsModal::Alert {
                                message: format!("{e}"),
                            });
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
        Action::SettingsDelete => {
            let idx = state.selected;
            if idx >= texts::MENU_ITEMS.len() {
                let file_idx = idx - texts::MENU_ITEMS.len();
                if file_idx < state.settings.recent_files.len() {
                    let path = &state.settings.recent_files[file_idx];
                    let is_active =
                        state.active_file.as_ref().and_then(|af| af.path.as_deref()) == Some(path);
                    if is_active {
                        state.modal = Some(SettingsModal::Alert {
                            message: texts::MSG_CANNOT_DELETE_OPEN.to_string(),
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

            if modal.mode == FileBrowserMode::ImportFile {
                state.modal = Some(SettingsModal::Alert {
                    message: texts::MSG_NOT_IMPLEMENTED.to_string(),
                });
            } else if creating {
                if file_handling::probe_path_writable(&path).is_ok() {
                    state.modal = Some(SettingsModal::PassphraseInput {
                        file_path: path,
                        file_data: None,
                        passphrase: String::new(),
                        cursor: 0,
                        error: None,
                    });
                } else {
                    state.modal = Some(SettingsModal::Alert {
                        message: texts::MSG_CANNOT_WRITE.to_string(),
                    });
                }
            } else {
                match file_handling::validate_encrypted_file(&path) {
                    Err(e) => {
                        state.modal = Some(SettingsModal::Alert {
                            message: format!("{e}"),
                        });
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

// ── Per-modal key handlers ──

fn handle_app_settings_key(key: event::KeyEvent, state: &mut SettingsState) {
    match Action::from_app_settings(&key) {
        Action::NavDown => {
            if let Some(SettingsModal::AppSettings {
                ref mut setting_idx,
            }) = state.modal
            {
                *setting_idx = (*setting_idx + 1).min(1);
            }
        }
        Action::NavUp => {
            if let Some(SettingsModal::AppSettings {
                ref mut setting_idx,
            }) = state.modal
            {
                *setting_idx = setting_idx.saturating_sub(1);
            }
        }
        Action::ToggleSetting => {
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
        Action::SaveSettings => {
            let _ = utils::save_settings(&state.settings);
            state.modal = Some(SettingsModal::Alert {
                message: texts::MSG_SETTINGS_SAVED.to_string(),
            });
        }
        Action::Cancel => {
            state.settings = utils::load_settings();
            state.modal = None;
        }
        _ => {}
    }
}

fn handle_delete_confirm_key(key: event::KeyEvent, state: &mut SettingsState) {
    match Action::from_confirm(&key) {
        Action::Yes => {
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
}

fn handle_change_pw_confirm_key(key: event::KeyEvent, state: &mut SettingsState) {
    match Action::from_confirm(&key) {
        Action::Yes => {
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
}

fn handle_change_pw_input_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut SettingsState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    match Action::from_change_pw_input(&key) {
        Action::NextField => {
            if let Some(SettingsModal::ChangePwInput {
                ref mut focused, ..
            }) = state.modal
            {
                *focused = (*focused + 1) % 3;
            }
        }
        Action::PrevField => {
            if let Some(SettingsModal::ChangePwInput {
                ref mut focused, ..
            }) = state.modal
            {
                *focused = focused.saturating_sub(1);
            }
        }
        Action::InsertChar(c) => {
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
        Action::Backspace => {
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
        Action::DeleteChar => {
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
                    0 if *current_cursor < current.len() => {
                        current.remove(*current_cursor);
                    }
                    1 if *new_cursor < new_pass.len() => {
                        new_pass.remove(*new_cursor);
                    }
                    2 if *confirm_cursor < confirm.len() => {
                        confirm.remove(*confirm_cursor);
                    }
                    _ => {}
                }
            }
        }
        Action::CursorLeft => {
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
        Action::CursorRight => {
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
        Action::CursorHome => {
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
        Action::CursorEnd => {
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
        Action::Cancel => {
            state.modal = None;
        }
        Action::Confirm => {
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
                    let is_ok = if current.as_bytes()
                        != state
                            .active_file
                            .as_ref()
                            .map(|af| af.passphrase.as_slice())
                            .unwrap_or(&[])
                    {
                        *error = Some(texts::MSG_CURRENT_PW_INCORRECT.to_string());
                        false
                    } else if new_pass.is_empty() || confirm.is_empty() {
                        *error = Some(texts::MSG_PW_EMPTY.to_string());
                        false
                    } else if new_pass != confirm {
                        *error = Some(texts::MSG_PW_MISMATCH.to_string());
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
                    if let (Some(ref path), Some(ref node)) = (&active_file.path, &active_file.node)
                    {
                        let new_pass_bytes = new_pass.into_bytes();
                        state.status = Some(texts::STATUS_SAVING);
                        terminal.draw(|f| render(f, f.area(), state))?;
                        match file_handling::save_tree(node, path, &new_pass_bytes) {
                            Err(e) => {
                                state.status = None;
                                state.modal = Some(SettingsModal::ChangePwInput {
                                    current: String::new(),
                                    current_cursor: 0,
                                    new_pass: String::new(),
                                    new_cursor: 0,
                                    confirm: String::new(),
                                    confirm_cursor: 0,
                                    focused: 0,
                                    error: Some(format!("{}{e}", texts::MSG_SAVE_FAILED)),
                                });
                            }
                            Ok(()) => {
                                state.status = None;
                                state.active_file.as_mut().unwrap().passphrase =
                                    new_pass_bytes.clone();
                                let mut tree_state =
                                    crate::tree::widget::TreeState::new(node.clone());
                                tree_state.active_file_path = Some(path.clone());
                                tree_state.active_file_passphrase = new_pass_bytes;
                                tree_state.display_full_path = state.settings.display_full_path;
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
    Ok(())
}

fn handle_confirm_no_passphrase_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut SettingsState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let file_path = match state.modal.take() {
        Some(SettingsModal::ConfirmNoPassphrase { file_path }) => file_path,
        _ => unreachable!(),
    };
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            state.status = Some(texts::STATUS_SAVING);
            terminal.draw(|f| render(f, f.area(), state))?;
            match file_handling::create_secret_file(&file_path, "") {
                Err(e) => {
                    state.status = None;
                    state.modal = Some(SettingsModal::Alert {
                        message: format!("{e}"),
                    });
                }
                Ok(node) => {
                    state.status = None;
                    let pp_bytes = Vec::new();
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
            }
        }
        _ => {
            state.modal = Some(SettingsModal::PassphraseInput {
                file_path,
                file_data: None,
                passphrase: String::new(),
                cursor: 0,
                error: None,
            });
        }
    }
    Ok(())
}

// ── Export/Import modal key handlers ──

fn handle_export_select_type_key(key: event::KeyEvent, state: &mut SettingsState) {
    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.modal = None;
        }
        Action::NavDown => {
            if let Some(SettingsModal::ExportSelectType { ref mut selected_type }) = state.modal {
                *selected_type = (*selected_type + 1) % 3;
            }
        }
        Action::NavUp => {
            if let Some(SettingsModal::ExportSelectType { ref mut selected_type }) = state.modal {
                *selected_type = (*selected_type + 2) % 3;
            }
        }
        Action::SelectType1 => {
            if let Some(SettingsModal::ExportSelectType { ref mut selected_type }) = state.modal {
                *selected_type = 0;
            }
        }
        Action::SelectType2 => {
            if let Some(SettingsModal::ExportSelectType { ref mut selected_type }) = state.modal {
                *selected_type = 1;
            }
        }
        Action::SelectType3 => {
            if let Some(SettingsModal::ExportSelectType { ref mut selected_type }) = state.modal {
                *selected_type = 2;
            }
        }
        Action::Confirm => {
            if let Some(SettingsModal::ExportSelectType { selected_type }) = state.modal.take() {
                let format = match selected_type {
                    0 => texts::FORMAT_JSON,
                    1 => texts::FORMAT_XML,
                    _ => texts::FORMAT_CSV,
                };
                state.modal = Some(SettingsModal::ExportConfirm {
                    export_type: format.to_string(),
                });
            }
        }
        _ => {}
    }
}

fn handle_export_confirm_key(key: event::KeyEvent, state: &mut SettingsState) {
    match Action::from_confirm(&key) {
        Action::Yes => {
            state.modal = Some(SettingsModal::Alert {
                message: texts::MSG_NOT_IMPLEMENTED.to_string(),
            });
        }
        _ => {
            state.modal = None;
        }
    }
}

// ── Modal key handling (Passphrase, Alert, FileBrowser) ──

fn handle_modal_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut SettingsState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    use SettingsModal::*;
    let kind: u8 = match state.modal.as_ref() {
        Some(AppSettings { .. }) => 0,
        Some(DeleteConfirm { .. }) => 1,
        Some(ChangePwConfirm) => 2,
        Some(ChangePwInput { .. }) => 3,
        Some(ConfirmNoPassphrase { .. }) => 4,
        Some(ExportSelectType { .. }) => 5,
        Some(ExportConfirm { .. }) => 6,
        _ => 255,
    };
    match kind {
        0 => {
            handle_app_settings_key(key, state);
            return Ok(());
        }
        1 => {
            handle_delete_confirm_key(key, state);
            return Ok(());
        }
        2 => {
            handle_change_pw_confirm_key(key, state);
            return Ok(());
        }
        3 => return handle_change_pw_input_key(key, state, terminal),
        4 => return handle_confirm_no_passphrase_key(key, state, terminal),
        5 => {
            handle_export_select_type_key(key, state);
            return Ok(());
        }
        6 => {
            handle_export_confirm_key(key, state);
            return Ok(());
        }
        _ => {}
    }

    let mut modal = match state.modal.take() {
        Some(m) => m,
        None => return Ok(()),
    };

    match Action::from_passphrase_input(&key) {
        Action::Cancel => {}
        Action::InsertChar(c) => match &mut modal {
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
            SettingsModal::Alert { .. } => {
                state.modal = Some(modal);
            }
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
        },
        Action::Backspace => match &mut modal {
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
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
        },
        Action::DeleteChar => match &mut modal {
            SettingsModal::PassphraseInput {
                passphrase, cursor, ..
            } => {
                if *cursor < passphrase.len() {
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
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
        },
        Action::CursorLeft => match &mut modal {
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
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
        },
        Action::CursorRight => match &mut modal {
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
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
        },
        Action::CursorHome => match &mut modal {
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
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
        },
        Action::CursorEnd => match &mut modal {
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
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
        },
        Action::Confirm => match modal {
            SettingsModal::PassphraseInput {
                file_path,
                file_data,
                passphrase,
                ..
            } => match file_data {
                Some(data) => {
                    state.status = Some(texts::STATUS_LOADING);
                    terminal.draw(|f| render(f, f.area(), state))?;
                    match file_handling::decrypt_to_node(&data, &passphrase) {
                        Err(e) => {
                            state.status = None;
                            state.modal = Some(SettingsModal::PassphraseInput {
                                file_path,
                                file_data: Some(data),
                                passphrase,
                                cursor: 0,
                                error: Some(format!("{}{e}", texts::MSG_WRONG_PW)),
                            });
                        }
                        Ok(node) => {
                            state.status = None;
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
                    }
                }
                None => {
                    if passphrase.is_empty() {
                        state.modal = Some(SettingsModal::ConfirmNoPassphrase { file_path });
                        return Ok(());
                    }
                    state.status = Some(texts::STATUS_SAVING);
                    terminal.draw(|f| render(f, f.area(), state))?;
                    match file_handling::create_secret_file(&file_path, &passphrase) {
                        Err(e) => {
                            state.status = None;
                            state.modal = Some(SettingsModal::Alert {
                                message: format!("{e}"),
                            });
                        }
                        Ok(node) => {
                            state.status = None;
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
                    }
                }
            },
            SettingsModal::Alert { .. } => {}
            SettingsModal::FileBrowser(_)
            | SettingsModal::AppSettings { .. }
            | SettingsModal::DeleteConfirm { .. }
            | SettingsModal::ChangePwConfirm
            | SettingsModal::ConfirmNoPassphrase { .. }
            | SettingsModal::ChangePwInput { .. }
            | SettingsModal::ExportSelectType { .. }
            | SettingsModal::ExportConfirm { .. } => unreachable!(),
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
        Some(SettingsModal::ConfirmNoPassphrase { .. }) => {
            render_confirm_no_passphrase_modal(frame, area)
        }
        Some(SettingsModal::ChangePwInput { .. }) => {
            render_change_pw_input_modal(frame, area, state)
        }
        Some(SettingsModal::ExportSelectType { .. }) => {
            render_export_select_type_modal(frame, area, state)
        }
        Some(SettingsModal::ExportConfirm { export_type }) => {
            render_export_confirm_modal(frame, area, export_type)
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

    let base_title = state
        .active_file
        .as_ref()
        .and_then(|af| af.path.as_deref())
        .map(|p| {
            if state.settings.display_full_path {
                format!("{} {} ", texts::TITLE_SETTINGS_PREFIX, p)
            } else {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or(std::borrow::Cow::Borrowed(p));
                format!("{} {} ", texts::TITLE_SETTINGS_PREFIX, name)
            }
        })
        .unwrap_or_else(|| texts::TITLE_SETTINGS_DEFAULT.to_string());

    let mut title_spans: Vec<Span> = Vec::new();
    if let Some(s) = state.status {
        use crate::theming::texts::{STATUS_LOADING, STATUS_SAVING};
        let el = match s {
            STATUS_LOADING => ElementStyle::StatusLoading,
            STATUS_SAVING => ElementStyle::StatusSaving,
            _ => ElementStyle::StatusInfo,
        };
        title_spans.push(Span::styled(format!(" {}  ", s), el.style()));
    }
    title_spans.push(Span::raw(base_title));

    let block = Block::default()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_style(ElementStyle::SettingsBlockBorder.style());

    let inner = block.inner(layout[0]);
    frame.render_widget(block, layout[0]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, item) in texts::MENU_ITEMS.iter().enumerate() {
        let is_selected = state.selected == i;
        let style = if is_selected {
            ElementStyle::SettingsSelected.style()
        } else {
            ElementStyle::SettingsNormal.style()
        };
        let prefix = if is_selected {
            Icon::SelectionArrow.as_str()
        } else {
            Icon::SelectionPad.as_str()
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, item),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::SEPARATOR_RECENT_FILES,
        ElementStyle::SettingsSectionHeader.style(),
    )));
    lines.push(Line::from(""));

    let menu_len = texts::MENU_ITEMS.len();
    for (i, file) in state.settings.recent_files.iter().enumerate() {
        let idx = menu_len + i;
        let is_selected = state.selected == idx;
        let style = if is_selected {
            ElementStyle::SettingsSelected.style()
        } else {
            ElementStyle::SettingsNormal.style()
        };
        let prefix = if is_selected {
            Icon::SelectionArrow.as_str()
        } else {
            Icon::SelectionPad.as_str()
        };
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
        .title(texts::SETTINGS_APP)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsOverlayBlock.style());

    let inner = block.inner(overlay_area);

    let rows = [
        (texts::SETTING_FULL_PATH, state.settings.display_full_path),
        (
            texts::SETTING_LAST_FILE,
            state.settings.open_last_opened_file_on_startup,
        ),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, (label, value)) in rows.iter().enumerate() {
        let is_selected = i == setting_idx;
        let style = if is_selected {
            ElementStyle::SettingsSelected.style()
        } else {
            ElementStyle::SettingsNormal.style()
        };
        let prefix = if is_selected {
            Icon::SelectionArrow.as_str()
        } else {
            Icon::SelectionPad.as_str()
        };
        let checkbox = if *value {
            Icon::CheckboxOn.as_str()
        } else {
            Icon::CheckboxOff.as_str()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{}{} ", prefix, checkbox), style),
            Span::styled(*label, style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_APP_SETTINGS,
        ElementStyle::SettingsHelp.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_delete_confirm_modal(frame: &mut Frame, area: Rect, path: &str) {
    let overlay_area = make_overlay(area, 7);
    let block = Block::default()
        .title(texts::SETTINGS_DELETE_CONFIRM)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsDeleteBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "{}{}{}",
                texts::PROMPT_DELETE_RECENT_START,
                path,
                texts::PROMPT_DELETE_RECENT_END
            ),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_DELETE_CONFIRM,
            ElementStyle::SettingsHelp.style(),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_change_pw_confirm_modal(frame: &mut Frame, area: Rect) {
    let overlay_area = make_overlay(area, 7);
    let block = Block::default()
        .title(texts::SETTINGS_CHANGE_PASSWORD)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsChangePwBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::PROMPT_CHANGE_PW,
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_CHANGE_PW_CONFIRM,
            ElementStyle::SettingsHelp.style(),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_confirm_no_passphrase_modal(frame: &mut Frame, area: Rect) {
    let overlay_area = make_overlay(area, 7);
    let block = Block::default()
        .title(texts::SETTINGS_NO_PASSPHRASE)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsNoPassBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::PROMPT_NO_PASSPHRASE,
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_NO_PASSPHRASE,
            ElementStyle::SettingsHelp.style(),
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
        .title(texts::SETTINGS_CHANGE_PASSWORD)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsChangePwBlock.style());

    let inner = block.inner(overlay_area);

    let fields = [
        (texts::LABEL_CURRENT_PW, &current, current_cursor),
        (texts::LABEL_NEW_PW, &new_pass, new_cursor),
        (texts::LABEL_CONFIRM_PW, &confirm, confirm_cursor),
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
                .and_then(|s: &str| s.chars().next())
                .map(|c: char| c.to_string())
                .unwrap_or_default();
            let after = &masked[pos + at.len()..];
            let cursor_char = if at.is_empty() { " " } else { &at };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}: ", label),
                    ElementStyle::SettingsFieldLabel.style(),
                ),
                Span::raw(before.to_string()),
                Span::styled(
                    cursor_char.to_string(),
                    ElementStyle::InvertedCursor.style(),
                ),
                Span::raw(after.to_string()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {}: ", label),
                    ElementStyle::SettingsFieldLabel.style(),
                ),
                Span::styled(masked, ElementStyle::SettingsFieldLabel.style()),
            ]));
        }
    }

    if let Some(ref err_msg) = error {
        lines.push(Line::from(Span::styled(
            format!(" {}", err_msg),
            ElementStyle::SettingsErrorBar.style(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_CHANGE_PW_INPUT,
        ElementStyle::SettingsHelp.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_export_select_type_modal(frame: &mut Frame, area: Rect, state: &SettingsState) {
    let selected_type = match &state.modal {
        Some(SettingsModal::ExportSelectType { selected_type }) => *selected_type,
        _ => 0,
    };

    let overlay_area = make_overlay(area, 10);
    let block = Block::default()
        .title(texts::EXPORT_SELECT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsOverlayBlock.style());

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, item) in texts::EXPORT_TYPE_ITEMS.iter().enumerate() {
        let is_selected = i == selected_type;
        let style = if is_selected {
            ElementStyle::SettingsSelected.style()
        } else {
            ElementStyle::SettingsNormal.style()
        };
        let prefix = if is_selected {
            Icon::SelectionArrow.as_str()
        } else {
            Icon::SelectionPad.as_str()
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, item),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_SELECT_FIELD_TYPE,
        ElementStyle::SettingsHelp.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_export_confirm_modal(frame: &mut Frame, area: Rect, export_type: &str) {
    let overlay_area = make_overlay(area, 7);
    let block = Block::default()
        .title(texts::EXPORT_CONFIRM_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsChangePwBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "{}{}{}",
                texts::PROMPT_EXPORT_CONFIRM_START,
                export_type,
                texts::PROMPT_EXPORT_CONFIRM_END
            ),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_CHANGE_PW_CONFIRM,
            ElementStyle::SettingsHelp.style(),
        )),
    ];

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
        }) => (file_path, passphrase, *cursor, error),
        _ => return,
    };

    let display_path = if state.settings.display_full_path {
        file_path.to_string()
    } else {
        std::path::Path::new(file_path.as_str())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string())
    };

    let overlay_area = make_overlay(area, if error.is_some() { 9 } else { 8 });

    let block = Block::default()
        .title(texts::SETTINGS_ENTER_PASSPHRASE)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsPassBlock.style());

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{} {}", texts::LABEL_FILE, display_path),
        ElementStyle::CursorLabel.style(),
    )));
    lines.push(Line::from(""));

    let display = "*".repeat(passphrase.len());
    let before = &display[..cursor.min(display.len())];
    let at = display
        .get(cursor..)
        .and_then(|s: &str| s.chars().next())
        .map(|c: char| c.to_string())
        .unwrap_or_default();
    let after = &display[cursor + at.len()..];

    let cursor_char = if at.is_empty() { " " } else { &at };

    lines.push(Line::from(vec![
        Span::styled(
            texts::LABEL_PASSPHRASE,
            ElementStyle::SettingsPassLabel.style(),
        ),
        Span::raw(before.to_string()),
        Span::styled(
            cursor_char.to_string(),
            ElementStyle::InvertedCursor.style(),
        ),
        Span::raw(after.to_string()),
    ]));

    if let Some(ref err_msg) = error {
        lines.push(Line::from(Span::styled(
            format!(" {}", err_msg),
            ElementStyle::SettingsErrorBar.style(),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::HINT_PASSPHRASE,
        ElementStyle::SettingsHelp.style(),
    )));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_alert_modal(frame: &mut Frame, area: Rect, message: &str) {
    let overlay_area = make_overlay(area, 6);
    let block = Block::default()
        .title(texts::SETTINGS_ALERT)
        .borders(Borders::ALL)
        .style(ElementStyle::SettingsAlertBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, ElementStyle::ConfirmMessage.style())),
        Line::from(""),
        Line::from(Span::styled(
            texts::HINT_ALERT,
            ElementStyle::SettingsHelp.style(),
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
        texts::STATUS_SETTINGS_LABEL,
        ElementStyle::SettingsStatusBar.style(),
    )]));
    frame.render_widget(Paragraph::new(text), area);
}

fn render_settings_help_bar(frame: &mut Frame, area: Rect) {
    let text = Text::from(Line::from(vec![Span::styled(
        texts::HINT_SETTINGS,
        ElementStyle::SettingsHelp.style(),
    )]));
    frame.render_widget(Paragraph::new(text), area);
}
