use std::borrow::Cow;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::{Frame, Terminal, backend::Backend};

use crate::cursor::{backspace_at, cursor_left, cursor_right, delete_at, insert_char_at};
use crate::file_browser::{
    self, FileBrowserAction, FileBrowserMode, FileBrowserSession, FileBrowserState,
};
use crate::file_handling;
use crate::generator::PasswordParams;
use crate::theming::Action;
use crate::theming::{
    ElementStyle, Icon, Palette, SPINNER_TICK, Spinner, Status, paint_background,
    render_status_bar, scheme_label, set_palette, texts, toggled_scheme,
};
use crate::tree::app_tree::SaveTarget;
use crate::vault::active_file::ActiveVaultState;
use crate::vault::utils::{self, Configuration};

enum HomeModal {
    FileBrowser(FileBrowserState),
    PassphraseInput {
        file_path: String,
        file_data: Vec<u8>,
        passphrase: String,
        cursor: usize,
        show: bool,
        error: Option<String>,
    },
    CreateSecretFile {
        file_path: String,
        passphrase: String,
        pass_cursor: usize,
        confirm: String,
        confirm_cursor: usize,
        focused: usize,
        show: bool,
        error: Option<String>,
    },
    Configuration {
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
        show: bool,
        error: Option<String>,
    },
    ExportSelectType {
        selected_type: usize,
    },
    ExportConfirm {
        export_type: String,
    },
}

pub struct HomeState {
    selected: usize,
    pub config: Configuration,
    pub quit: bool,
    pub active_vault: Option<ActiveVaultState>,
    modal: Option<HomeModal>,
    /// Title-bar status of a blocking operation, if any.
    pub status: Option<Status>,
    /// Frame counter for the animated busy glyph shown next to [`HomeState::status`].
    spinner: Spinner,
    /// In-memory memory shared by successive file browser openings.
    browser_session: FileBrowserSession,
    /// Password-generator settings for this app run; seeded into each opened
    /// tree and copied back when it exits. Never persisted to the config file.
    pub generator: PasswordParams,
}

impl Default for HomeState {
    fn default() -> Self {
        Self::new()
    }
}

impl HomeState {
    pub fn new() -> Self {
        let config = utils::load_config();
        // Apply the persisted colour scheme before the first frame is drawn.
        set_palette(Palette::from_name(&config.palette_name));
        let modal = if config.open_last_opened_file_on_startup {
            config.last_opened_file.as_ref().and_then(|path| {
                file_handling::validate_encrypted_file(path)
                    .ok()
                    .map(|data| HomeModal::PassphraseInput {
                        file_path: path.clone(),
                        file_data: data,
                        passphrase: String::new(),
                        cursor: 0,
                        show: false,
                        error: None,
                    })
            })
        } else {
            None
        };
        Self {
            selected: 0,
            config,
            quit: false,
            active_vault: None,
            modal,
            status: None,
            spinner: Spinner::default(),
            browser_session: FileBrowserSession::default(),
            generator: PasswordParams::default(),
        }
    }

    fn total_navigable(&self) -> usize {
        texts::MENU_ITEMS.len() + self.config.recent_files.len()
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
    let mut state = HomeState::new();
    loop {
        if state.quit {
            break Ok(());
        }

        terminal.draw(|f| {
            let area = f.area();
            render(f, area, &mut state);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            let is_file_browser = state
                .modal
                .as_ref()
                .is_some_and(|m| matches!(m, HomeModal::FileBrowser(_)));

            if state.modal.is_none() {
                handle_home_key(key, &mut state, terminal)?;
            } else if is_file_browser {
                handle_file_browser_key(key, &mut state, terminal)?;
            } else {
                handle_modal_key(key, &mut state, terminal)?;
            }
        }
    }
}

// ── Blocking work with an animated title-bar status ──

/// Runs `work` on a worker thread while an animated `status` shows in the title
/// bar.
///
/// The UI thread keeps redrawing every [`SPINNER_TICK`] until the worker
/// finishes, draining any input so a stray key cannot trigger a second action
/// once the wait ends. If a draw fails mid-wait the worker keeps running: the
/// save/decrypt is *not* cancelled.
///
/// The status is cleared on return (success or error) without a final draw; the
/// caller sets the next modal and the outer loop's top-of-iteration draw renders
/// the real result, avoiding a flash of the wrong screen.
fn run_with_spinner<B, T, F>(
    terminal: &mut Terminal<B>,
    state: &mut HomeState,
    status: Status,
    work: F,
) -> io::Result<T>
where
    B: Backend,
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    state.status = Some(status);
    terminal.draw(|f| render(f, f.area(), state))?;

    let handle = std::thread::Builder::new()
        .spawn(work)
        .map_err(io::Error::other)?;

    while !handle.is_finished() {
        state.spinner.advance();
        terminal.draw(|f| render(f, f.area(), state))?;
        if event::poll(SPINNER_TICK)? {
            // Drain every pending event so none is reinterpreted afterwards.
            while event::poll(Duration::ZERO)? {
                let _ = event::read()?;
            }
        }
    }

    let result = handle
        .join()
        .map_err(|_| io::Error::other("blocking worker thread panicked"))?;
    state.status = None;
    Ok(result)
}

// ── Settings menu key handling (no modal active) ──

fn handle_home_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    match Action::from_home(&key) {
        Action::Quit => state.quit = true,
        Action::NavDown => state.select_next(),
        Action::NavUp => state.select_prev(),
        Action::ReturnToMenu => {
            reopen_tree(state, terminal)?;
        }
        Action::Back => {
            if state
                .active_vault
                .as_ref()
                .and_then(|af| af.node.as_ref())
                .is_some()
            {
                reopen_tree(state, terminal)?;
            } else {
                state.quit = true;
            }
        }
        Action::Confirm => {
            let idx = state.selected;
            if idx == 0 {
                let mut fb =
                    FileBrowserState::new(FileBrowserMode::OpenFile, &state.browser_session);
                fb.set_active_vault_path(
                    state
                        .active_vault
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(PathBuf::from)),
                );
                state.modal = Some(HomeModal::FileBrowser(fb));
            } else if idx == 1 {
                let mut fb =
                    FileBrowserState::new(FileBrowserMode::CreateFile, &state.browser_session);
                fb.set_active_vault_path(
                    state
                        .active_vault
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(PathBuf::from)),
                );
                state.modal = Some(HomeModal::FileBrowser(fb));
            } else if idx == 2 {
                state.modal = Some(HomeModal::Configuration { setting_idx: 0 });
            } else if idx == 3 {
                if state.active_vault.is_some() {
                    state.modal = Some(HomeModal::ChangePwConfirm);
                } else {
                    state.modal = Some(HomeModal::Alert {
                        message: texts::MSG_NO_OPEN_FILE.to_string(),
                    });
                }
            } else if idx == 4 {
                if state.active_vault.is_some() {
                    state.modal = Some(HomeModal::ExportSelectType { selected_type: 0 });
                } else {
                    state.modal = Some(HomeModal::Alert {
                        message: texts::MSG_NO_OPEN_FILE.to_string(),
                    });
                }
            } else if idx == 5 {
                let mut fb =
                    FileBrowserState::new(FileBrowserMode::ImportFile, &state.browser_session);
                fb.set_active_vault_path(
                    state
                        .active_vault
                        .as_ref()
                        .and_then(|af| af.path.as_ref().map(PathBuf::from)),
                );
                state.modal = Some(HomeModal::FileBrowser(fb));
            } else {
                let file_idx = idx - texts::MENU_ITEMS.len();
                if file_idx < state.config.recent_files.len() {
                    let file_path: String = state.config.recent_files[file_idx].clone();

                    if let Some(ref active_vault) = state.active_vault
                        && active_vault.path.as_deref() == Some(&file_path)
                        && let Some(ref node) = active_vault.node
                    {
                        let passphrase = active_vault.passphrase.clone();
                        let mut tree_state = crate::tree::widget::TreeState::new(node.clone());
                        tree_state.display_full_path = state.config.display_full_path;
                        tree_state.generator = state.generator;
                        run_tree(
                            state,
                            terminal,
                            tree_state,
                            Some(SaveTarget {
                                path: file_path,
                                passphrase,
                            }),
                        )?;
                        return Ok(());
                    }

                    match file_handling::validate_encrypted_file(&file_path) {
                        Err(e) => {
                            state.modal = Some(HomeModal::Alert {
                                message: format!("{e}"),
                            });
                        }
                        Ok(data) => {
                            state.modal = Some(HomeModal::PassphraseInput {
                                file_path,
                                file_data: data,
                                passphrase: String::new(),
                                cursor: 0,
                                show: false,
                                error: None,
                            });
                        }
                    }
                }
            }
        }
        Action::ConfigDelete => {
            let idx = state.selected;
            if idx >= texts::MENU_ITEMS.len() {
                let file_idx = idx - texts::MENU_ITEMS.len();
                if file_idx < state.config.recent_files.len() {
                    let path = &state.config.recent_files[file_idx];
                    let is_active = state
                        .active_vault
                        .as_ref()
                        .and_then(|af| af.path.as_deref())
                        == Some(path);
                    if is_active {
                        state.modal = Some(HomeModal::Alert {
                            message: texts::MSG_CANNOT_DELETE_OPEN.to_string(),
                        });
                    } else {
                        state.modal = Some(HomeModal::DeleteConfirm { path: path.clone() });
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn reopen_tree<B: Backend>(state: &mut HomeState, terminal: &mut Terminal<B>) -> io::Result<()> {
    if let Some(ref active_vault) = state.active_vault
        && let Some(ref node) = active_vault.node
    {
        let passphrase = active_vault.passphrase.clone();
        let target = active_vault
            .path
            .clone()
            .map(|path| SaveTarget { path, passphrase });
        let mut tree_state = crate::tree::widget::TreeState::new(node.clone());
        tree_state.display_full_path = state.config.display_full_path;
        tree_state.generator = state.generator;
        run_tree(state, terminal, tree_state, target)?;
    }
    Ok(())
}

/// Runs the tree and hands its final root back to the active vault.
///
/// On a normal exit the autosave worker may have written a newer root to disk
/// than the one `HomeState` originally decrypted; keeping the in-memory home
/// copy in sync prevents later home operations (change password, import) from
/// overwriting those edits with stale data. A force-quit ([`ExitStatus::Unsaved`])
/// instead drops the active vault, so no later home action can persist or
/// overwrite with the discarded in-memory tree.
fn run_tree<B: Backend>(
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
    mut tree_state: crate::tree::widget::TreeState,
    target: Option<SaveTarget>,
) -> io::Result<()> {
    use crate::tree::app_tree::ExitStatus;

    if let Some(target) = &target {
        tree_state.active_vault_path = Some(target.path.clone());
    }
    let result = crate::tree::app_tree::run_app(terminal, &mut tree_state, target);

    // Generator settings are per app run: copy them back even when the tree
    // exits with an error or the user discards changes.
    state.generator = tree_state.generator;
    let return_to_menu = tree_state.return_to_menu;
    match result {
        Ok(ExitStatus::Saved) => {
            if let Some(active) = state.active_vault.as_mut() {
                active.node = Some(tree_state.root);
            }
            if !return_to_menu {
                state.quit = true;
            }
            Ok(())
        }
        Ok(ExitStatus::Unsaved) => {
            state.active_vault = None;
            if !return_to_menu {
                state.quit = true;
            }
            Ok(())
        }
        // Sync even on error: a later retry from Home must not start from a
        // stale copy that could overwrite edits already written by the worker.
        Err(error) => {
            if let Some(active) = state.active_vault.as_mut() {
                active.node = Some(tree_state.root);
            }
            Err(error)
        }
    }
}

// ── File browser key handling ──

fn handle_file_browser_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let mut modal = match state.modal.take() {
        Some(HomeModal::FileBrowser(b)) => b,
        _ => return Ok(()),
    };

    let action = file_browser::handle_key(key, &mut modal);
    state.browser_session = modal.session();

    match action {
        None => {
            state.modal = Some(HomeModal::FileBrowser(modal));
        }
        Some(FileBrowserAction::Cancelled) => {}
        Some(FileBrowserAction::Renamed { from, to }) => {
            if let Some(ref active_vault) = state.active_vault
                && active_vault.path.as_deref() == Some(&from)
            {
                state.active_vault.as_mut().unwrap().path = Some(to.clone());
            }
            if state.config.last_opened_file.as_deref() == Some(&from) {
                state.config.last_opened_file = Some(to.clone());
            }
            for path in state.config.recent_files.iter_mut() {
                if *path == from {
                    *path = to.clone();
                    break;
                }
            }
            let _ = utils::save_config(&state.config);
            state.modal = Some(HomeModal::FileBrowser(modal));
        }
        Some(FileBrowserAction::Selected(path)) => {
            let creating = modal.mode == FileBrowserMode::CreateFile;

            if modal.mode == FileBrowserMode::ImportFile {
                match file_handling::import::import_tree(&path) {
                    Ok(imported_node) => {
                        let save_target = if let Some(active_vault) = state.active_vault.as_mut() {
                            match active_vault.node.as_mut() {
                                Some(root) => {
                                    let filename = std::path::Path::new(&path)
                                        .file_stem()
                                        .map(|s| s.to_string_lossy().to_string())
                                        .unwrap_or_else(|| texts::IMPORT_NAME_FALLBACK.to_string());
                                    let date_time =
                                        chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();
                                    let mut wrapper = crate::tree::node::Node::new(format!(
                                        "imported_{}_{}",
                                        filename, date_time
                                    ));
                                    wrapper.children = vec![imported_node];
                                    root.add_child(wrapper);

                                    match active_vault.path.clone() {
                                        Some(p) => {
                                            Some((p, active_vault.passphrase.clone(), root.clone()))
                                        }
                                        None => {
                                            state.modal = Some(HomeModal::Alert {
                                                message: texts::MSG_NO_FILE_PATH.to_string(),
                                            });
                                            return Ok(());
                                        }
                                    }
                                }
                                None => None,
                            }
                        } else {
                            state.modal = Some(HomeModal::Alert {
                                message: texts::MSG_IMPORT_NO_OPEN_FILE.to_string(),
                            });
                            return Ok(());
                        };

                        if let Some((save_path, save_pass, save_node)) = save_target {
                            let outcome =
                                run_with_spinner(terminal, state, Status::Saving, move || {
                                    file_handling::save_tree(&save_node, &save_path, &save_pass)
                                })?;
                            match outcome {
                                Err(e) => {
                                    state.modal = Some(HomeModal::Alert {
                                        message: format!("{}{}", texts::MSG_SAVE_FAILED, e),
                                    });
                                }
                                Ok(()) => {
                                    state.modal = Some(HomeModal::Alert {
                                        message: texts::MSG_IMPORT_SUCCESS.to_string(),
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => {
                        state.modal = Some(HomeModal::Alert {
                            message: format!("{e}"),
                        });
                    }
                }
            } else if creating {
                if std::path::Path::new(&path).exists() {
                    state.modal = Some(HomeModal::Alert {
                        message: texts::MSG_FILE_EXISTS.to_string(),
                    });
                } else if file_handling::probe_path_writable(&path).is_ok() {
                    state.modal = Some(HomeModal::CreateSecretFile {
                        file_path: path,
                        passphrase: String::new(),
                        pass_cursor: 0,
                        confirm: String::new(),
                        confirm_cursor: 0,
                        focused: 0,
                        show: false,
                        error: None,
                    });
                } else {
                    state.modal = Some(HomeModal::Alert {
                        message: texts::MSG_CANNOT_WRITE.to_string(),
                    });
                }
            } else {
                match file_handling::validate_encrypted_file(&path) {
                    Err(e) => {
                        state.modal = Some(HomeModal::Alert {
                            message: format!("{e}"),
                        });
                    }
                    Ok(data) => {
                        state.modal = Some(HomeModal::PassphraseInput {
                            file_path: path,
                            file_data: data,
                            passphrase: String::new(),
                            cursor: 0,
                            show: false,
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

fn handle_config_key(key: event::KeyEvent, state: &mut HomeState) {
    match Action::from_config(&key) {
        Action::NavDown => {
            if let Some(HomeModal::Configuration {
                ref mut setting_idx,
            }) = state.modal
            {
                *setting_idx = (*setting_idx + 1).min(2);
            }
        }
        Action::NavUp => {
            if let Some(HomeModal::Configuration {
                ref mut setting_idx,
            }) = state.modal
            {
                *setting_idx = setting_idx.saturating_sub(1);
            }
        }
        Action::ToggleSetting => {
            let idx = match state.modal.as_ref() {
                Some(HomeModal::Configuration { setting_idx }) => *setting_idx,
                _ => return,
            };
            match idx {
                0 => state.config.display_full_path = !state.config.display_full_path,
                1 => {
                    state.config.open_last_opened_file_on_startup =
                        !state.config.open_last_opened_file_on_startup;
                }
                _ => {
                    // Apply the scheme the moment it changes, so the user sees
                    // the new colours while the modal stays open; it is still
                    // persisted only on save (`Enter`), like the other settings.
                    state.config.palette_name =
                        toggled_scheme(&state.config.palette_name).to_string();
                    set_palette(Palette::from_name(&state.config.palette_name));
                }
            }
        }
        Action::SaveConfig => {
            let _ = utils::save_config(&state.config);
            state.modal = Some(HomeModal::Alert {
                message: texts::MSG_CONFIG_SAVED.to_string(),
            });
        }
        Action::Cancel => {
            state.config = utils::load_config();
            state.modal = None;
        }
        _ => {}
    }
}

fn handle_delete_confirm_key(key: event::KeyEvent, state: &mut HomeState) {
    match Action::from_confirm(&key) {
        Action::Yes => {
            if let Some(HomeModal::DeleteConfirm { path }) = state.modal.take() {
                state.config.recent_files.retain(|f| f != &path);
                if state.config.last_opened_file.as_deref() == Some(&path) {
                    state.config.last_opened_file = None;
                }
                let _ = utils::save_config(&state.config);
            }
        }
        _ => {
            state.modal = None;
        }
    }
}

fn handle_change_pw_confirm_key(key: event::KeyEvent, state: &mut HomeState) {
    match Action::from_confirm(&key) {
        Action::Yes => {
            state.modal = Some(HomeModal::ChangePwInput {
                current: String::new(),
                current_cursor: 0,
                new_pass: String::new(),
                new_cursor: 0,
                confirm: String::new(),
                confirm_cursor: 0,
                focused: 0,
                show: false,
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
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    match Action::from_change_pw_input(&key) {
        Action::NextField => {
            if let Some(HomeModal::ChangePwInput {
                ref mut focused, ..
            }) = state.modal
            {
                *focused = (*focused + 1) % 3;
            }
        }
        Action::PrevField => {
            if let Some(HomeModal::ChangePwInput {
                ref mut focused, ..
            }) = state.modal
            {
                *focused = focused.saturating_sub(1);
            }
        }
        Action::ToggleShowPassword => {
            if let Some(HomeModal::ChangePwInput { ref mut show, .. }) = state.modal {
                *show = !*show;
            }
        }
        Action::InsertChar(c) => {
            if let Some(HomeModal::ChangePwInput {
                ref mut current,
                ref mut current_cursor,
                ref mut new_pass,
                ref mut new_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ref mut error,
                ..
            }) = state.modal
            {
                *error = None;
                match *focused {
                    0 => insert_char_at(current, current_cursor, c),
                    1 => insert_char_at(new_pass, new_cursor, c),
                    _ => insert_char_at(confirm, confirm_cursor, c),
                }
            }
        }
        Action::Backspace => {
            if let Some(HomeModal::ChangePwInput {
                ref mut current,
                ref mut current_cursor,
                ref mut new_pass,
                ref mut new_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ref mut error,
                ..
            }) = state.modal
            {
                *error = None;
                match *focused {
                    0 if *current_cursor > 0 => {
                        *current_cursor = backspace_at(current, *current_cursor);
                    }
                    1 if *new_cursor > 0 => {
                        *new_cursor = backspace_at(new_pass, *new_cursor);
                    }
                    2 if *confirm_cursor > 0 => {
                        *confirm_cursor = backspace_at(confirm, *confirm_cursor);
                    }
                    _ => {}
                }
            }
        }
        Action::DeleteChar => {
            if let Some(HomeModal::ChangePwInput {
                ref mut current,
                ref mut current_cursor,
                ref mut new_pass,
                ref mut new_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ref mut error,
                ..
            }) = state.modal
            {
                *error = None;
                match *focused {
                    0 => delete_at(current, *current_cursor),
                    1 => delete_at(new_pass, *new_cursor),
                    2 => delete_at(confirm, *confirm_cursor),
                    _ => {}
                }
            }
        }
        Action::CursorLeft => {
            if let Some(HomeModal::ChangePwInput {
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
                    0 => *current_cursor = cursor_left(current, *current_cursor),
                    1 => *new_cursor = cursor_left(new_pass, *new_cursor),
                    2 => *confirm_cursor = cursor_left(confirm, *confirm_cursor),
                    _ => {}
                }
            }
        }
        Action::CursorRight => {
            if let Some(HomeModal::ChangePwInput {
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
                    0 => *current_cursor = cursor_right(current, *current_cursor),
                    1 => *new_cursor = cursor_right(new_pass, *new_cursor),
                    2 => *confirm_cursor = cursor_right(confirm, *confirm_cursor),
                    _ => {}
                }
            }
        }
        Action::CursorHome => {
            if let Some(HomeModal::ChangePwInput {
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
            if let Some(HomeModal::ChangePwInput {
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
                Some(HomeModal::ChangePwInput {
                    current,
                    current_cursor,
                    new_pass,
                    new_cursor,
                    confirm,
                    confirm_cursor,
                    focused,
                    show,
                    error,
                }) => HomeModal::ChangePwInput {
                    current,
                    current_cursor,
                    new_pass,
                    new_cursor,
                    confirm,
                    confirm_cursor,
                    focused,
                    show,
                    error,
                },
                _ => return Ok(()),
            };
            let (_current, new_pass, _confirm, has_error) = match &mut modal {
                HomeModal::ChangePwInput {
                    current,
                    new_pass,
                    confirm,
                    error,
                    ..
                } => {
                    let is_ok = if current.as_bytes()
                        != state
                            .active_vault
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
                _ => return Ok(()),
            };
            if has_error {
                state.modal = Some(modal);
            } else {
                // Save with new passphrase and reopen
                if let Some(active_vault) = state.active_vault.clone()
                    && let (Some(path), Some(node)) =
                        (active_vault.path.clone(), active_vault.node.clone())
                {
                    let new_pass_bytes = zeroize::Zeroizing::new(new_pass.into_bytes());
                    let save_node = node.clone();
                    let save_path = path.clone();
                    let (outcome, new_pass_bytes) =
                        run_with_spinner(terminal, state, Status::Saving, move || {
                            let outcome =
                                file_handling::save_tree(&save_node, &save_path, &new_pass_bytes);
                            (outcome, new_pass_bytes)
                        })?;
                    match outcome {
                        Err(e) => {
                            state.modal = Some(HomeModal::ChangePwInput {
                                current: String::new(),
                                current_cursor: 0,
                                new_pass: String::new(),
                                new_cursor: 0,
                                confirm: String::new(),
                                confirm_cursor: 0,
                                focused: 0,
                                show: false,
                                error: Some(format!("{}{e}", texts::MSG_SAVE_FAILED)),
                            });
                        }
                        Ok(()) => {
                            state.active_vault.as_mut().unwrap().passphrase =
                                new_pass_bytes.clone();
                            let mut tree_state = crate::tree::widget::TreeState::new(node.clone());
                            tree_state.display_full_path = state.config.display_full_path;
                            tree_state.generator = state.generator;
                            run_tree(
                                state,
                                terminal,
                                tree_state,
                                Some(SaveTarget {
                                    path: path.clone(),
                                    passphrase: new_pass_bytes,
                                }),
                            )?;
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
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let file_path = match state.modal.take() {
        Some(HomeModal::ConfirmNoPassphrase { file_path }) => file_path,
        _ => return Ok(()),
    };
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
            let (outcome, file_path) =
                run_with_spinner(terminal, state, Status::Saving, move || {
                    let outcome = file_handling::create_secret_file(&file_path, "");
                    (outcome, file_path)
                })?;
            match outcome {
                Err(e) => {
                    state.modal = Some(HomeModal::Alert {
                        message: format!("{e}"),
                    });
                }
                Ok(node) => {
                    state.modal = None;
                    let pp_bytes = zeroize::Zeroizing::new(Vec::new());
                    let active_vault = ActiveVaultState {
                        path: Some(file_path.clone()),
                        passphrase: pp_bytes.clone(),
                        node: Some(node.clone()),
                    };
                    state.active_vault = Some(active_vault);

                    let _ = utils::push_recent_secret_file(&mut state.config, &file_path);

                    let mut tree_state = crate::tree::widget::TreeState::new(node);
                    tree_state.display_full_path = state.config.display_full_path;
                    tree_state.generator = state.generator;
                    run_tree(
                        state,
                        terminal,
                        tree_state,
                        Some(SaveTarget {
                            path: file_path,
                            passphrase: pp_bytes,
                        }),
                    )?;
                }
            }
        }
        _ => {
            state.modal = Some(HomeModal::CreateSecretFile {
                file_path,
                passphrase: String::new(),
                pass_cursor: 0,
                confirm: String::new(),
                confirm_cursor: 0,
                focused: 0,
                show: false,
                error: None,
            });
        }
    }
    Ok(())
}

// ── Export/Import modal key handlers ──

fn handle_export_select_type_key(key: event::KeyEvent, state: &mut HomeState) {
    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.modal = None;
        }
        Action::NavDown => {
            if let Some(HomeModal::ExportSelectType {
                ref mut selected_type,
            }) = state.modal
            {
                *selected_type = (*selected_type + 1) % 3;
            }
        }
        Action::NavUp => {
            if let Some(HomeModal::ExportSelectType {
                ref mut selected_type,
            }) = state.modal
            {
                *selected_type = (*selected_type + 2) % 3;
            }
        }
        Action::SelectIndex(i) if i < 3 => {
            if let Some(HomeModal::ExportSelectType {
                ref mut selected_type,
            }) = state.modal
            {
                *selected_type = i;
            }
        }
        Action::Confirm => {
            if let Some(HomeModal::ExportSelectType { selected_type }) = state.modal.take() {
                let format = match selected_type {
                    0 => texts::FORMAT_JSON,
                    1 => texts::FORMAT_XML,
                    _ => texts::FORMAT_CSV,
                };
                state.modal = Some(HomeModal::ExportConfirm {
                    export_type: format.to_string(),
                });
            }
        }
        _ => {}
    }
}

fn handle_export_confirm_key(key: event::KeyEvent, state: &mut HomeState) {
    match Action::from_confirm(&key) {
        Action::Yes => {
            let export_type = match state.modal.take() {
                Some(HomeModal::ExportConfirm { export_type }) => export_type,
                _ => return,
            };

            let af = match state.active_vault.clone() {
                Some(af) => af,
                None => return,
            };

            let path = match af.path {
                Some(p) => p,
                None => return,
            };

            let node = match af.node {
                Some(n) => n,
                None => return,
            };

            let format = match export_type.as_str() {
                "JSON" => file_handling::export::ExportFormat::Json,
                "XML" => file_handling::export::ExportFormat::Xml,
                _ => file_handling::export::ExportFormat::Csv,
            };

            match file_handling::export::export_tree(&node, &path, format) {
                Ok(out_path) => {
                    state.modal = Some(HomeModal::Alert {
                        message: format!("{}{}", texts::MSG_EXPORT_SUCCESS, out_path),
                    });
                }
                Err(e) => {
                    state.modal = Some(HomeModal::Alert {
                        message: format!("{e}"),
                    });
                }
            }
        }
        _ => {
            state.modal = None;
        }
    }
}

// ── Modal key handling (Passphrase, Alert, FileBrowser) ──

fn handle_modal_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let is_passphrase = matches!(
        state.modal,
        Some(HomeModal::PassphraseInput { .. }) | Some(HomeModal::Alert { .. })
    );
    if is_passphrase {
        return handle_passphrase_key(key, state, terminal);
    }

    let is_app_settings = matches!(state.modal, Some(HomeModal::Configuration { .. }));
    if is_app_settings {
        handle_config_key(key, state);
        return Ok(());
    }

    let is_delete_confirm = matches!(state.modal, Some(HomeModal::DeleteConfirm { .. }));
    if is_delete_confirm {
        handle_delete_confirm_key(key, state);
        return Ok(());
    }

    let is_change_pw_confirm = matches!(state.modal, Some(HomeModal::ChangePwConfirm));
    if is_change_pw_confirm {
        handle_change_pw_confirm_key(key, state);
        return Ok(());
    }

    let is_change_pw_input = matches!(state.modal, Some(HomeModal::ChangePwInput { .. }));
    if is_change_pw_input {
        return handle_change_pw_input_key(key, state, terminal);
    }

    let is_create_secret = matches!(state.modal, Some(HomeModal::CreateSecretFile { .. }));
    if is_create_secret {
        return handle_create_secret_file_key(key, state, terminal);
    }

    let is_confirm_no_passphrase =
        matches!(state.modal, Some(HomeModal::ConfirmNoPassphrase { .. }));
    if is_confirm_no_passphrase {
        return handle_confirm_no_passphrase_key(key, state, terminal);
    }

    let is_export_select = matches!(state.modal, Some(HomeModal::ExportSelectType { .. }));
    if is_export_select {
        handle_export_select_type_key(key, state);
        return Ok(());
    }

    let is_export_confirm = matches!(state.modal, Some(HomeModal::ExportConfirm { .. }));
    if is_export_confirm {
        handle_export_confirm_key(key, state);
        return Ok(());
    }

    // No modal or unknown modal: nothing to do.
    Ok(())
}

fn handle_passphrase_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let mut modal = match state.modal.take() {
        Some(m) => m,
        None => return Ok(()),
    };

    match Action::from_passphrase_input(&key) {
        Action::Cancel => {}
        Action::ToggleShowPassword => {
            if let HomeModal::PassphraseInput { show, .. } = &mut modal {
                *show = !*show;
            }
            state.modal = Some(modal);
        }
        Action::InsertChar(c) => {
            if let HomeModal::PassphraseInput {
                passphrase,
                cursor,
                error,
                ..
            } = &mut modal
            {
                insert_char_at(passphrase, cursor, c);
                *error = None;
            }
            state.modal = Some(modal);
        }
        Action::Backspace => {
            if let HomeModal::PassphraseInput {
                passphrase, cursor, ..
            } = &mut modal
                && *cursor > 0
            {
                *cursor = backspace_at(passphrase, *cursor);
            }
            state.modal = Some(modal);
        }
        Action::DeleteChar => {
            if let HomeModal::PassphraseInput {
                passphrase, cursor, ..
            } = &mut modal
            {
                delete_at(passphrase, *cursor);
            }
            state.modal = Some(modal);
        }
        Action::CursorLeft => {
            if let HomeModal::PassphraseInput {
                cursor, passphrase, ..
            } = &mut modal
            {
                *cursor = cursor_left(passphrase, *cursor);
            }
            state.modal = Some(modal);
        }
        Action::CursorRight => {
            if let HomeModal::PassphraseInput {
                cursor, passphrase, ..
            } = &mut modal
            {
                *cursor = cursor_right(passphrase, *cursor);
            }
            state.modal = Some(modal);
        }
        Action::CursorHome => {
            if let HomeModal::PassphraseInput { cursor, .. } = &mut modal {
                *cursor = 0;
            }
            state.modal = Some(modal);
        }
        Action::CursorEnd => {
            if let HomeModal::PassphraseInput {
                cursor, passphrase, ..
            } = &mut modal
            {
                *cursor = passphrase.len();
            }
            state.modal = Some(modal);
        }
        Action::Confirm => match modal {
            HomeModal::PassphraseInput {
                file_path,
                file_data,
                passphrase,
                show,
                ..
            } => {
                let (outcome, file_data, passphrase) =
                    run_with_spinner(terminal, state, Status::Loading, move || {
                        let outcome = file_handling::decrypt_to_node(&file_data, &passphrase);
                        (outcome, file_data, passphrase)
                    })?;
                match outcome {
                    Err(e) => {
                        state.modal = Some(HomeModal::PassphraseInput {
                            file_path,
                            file_data,
                            passphrase,
                            cursor: 0,
                            show,
                            error: Some(format!("{}{e}", texts::MSG_WRONG_PW)),
                        });
                    }
                    Ok(node) => {
                        let pp_bytes = zeroize::Zeroizing::new(passphrase.into_bytes());
                        let active_vault = ActiveVaultState {
                            path: Some(file_path.clone()),
                            passphrase: pp_bytes.clone(),
                            node: Some(node.clone()),
                        };
                        state.active_vault = Some(active_vault);

                        let _ = utils::push_recent_secret_file(&mut state.config, &file_path);

                        let mut tree_state = crate::tree::widget::TreeState::new(node);
                        tree_state.display_full_path = state.config.display_full_path;
                        tree_state.generator = state.generator;
                        run_tree(
                            state,
                            terminal,
                            tree_state,
                            Some(SaveTarget {
                                path: file_path,
                                passphrase: pp_bytes,
                            }),
                        )?;
                    }
                }
            }
            HomeModal::Alert { .. } => {}
            _ => {}
        },
        _ => {
            state.modal = Some(modal);
        }
    }
    Ok(())
}

/// Handles keys for the dedicated "create new secret file" dialog.
///
/// The dialog holds a passphrase and its confirmation. `Ctrl+S` reveals both;
/// `Enter` validates, then creates the encrypted file and launches the tree.
/// An all-empty pair falls through to the no-passphrase confirmation.
fn handle_create_secret_file_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    match Action::from_create_secret_file(&key) {
        Action::NextField => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut focused, ..
            }) = state.modal
            {
                *focused = (*focused + 1) % 2;
            }
        }
        Action::PrevField => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut focused, ..
            }) = state.modal
            {
                *focused = focused.saturating_sub(1);
            }
        }
        Action::ToggleShowPassword => {
            if let Some(HomeModal::CreateSecretFile { ref mut show, .. }) = state.modal {
                *show = !*show;
            }
        }
        Action::InsertChar(c) => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut passphrase,
                ref mut pass_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ref mut error,
                ..
            }) = state.modal
            {
                *error = None;
                match *focused {
                    0 => insert_char_at(passphrase, pass_cursor, c),
                    _ => insert_char_at(confirm, confirm_cursor, c),
                }
            }
        }
        Action::Backspace => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut passphrase,
                ref mut pass_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ref mut error,
                ..
            }) = state.modal
            {
                *error = None;
                match *focused {
                    0 if *pass_cursor > 0 => {
                        *pass_cursor = backspace_at(passphrase, *pass_cursor);
                    }
                    1 if *confirm_cursor > 0 => {
                        *confirm_cursor = backspace_at(confirm, *confirm_cursor);
                    }
                    _ => {}
                }
            }
        }
        Action::DeleteChar => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut passphrase,
                ref mut pass_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ref mut error,
                ..
            }) = state.modal
            {
                *error = None;
                match *focused {
                    0 => delete_at(passphrase, *pass_cursor),
                    _ => delete_at(confirm, *confirm_cursor),
                }
            }
        }
        Action::CursorLeft => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut passphrase,
                ref mut pass_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ..
            }) = state.modal
            {
                match *focused {
                    0 => *pass_cursor = cursor_left(passphrase, *pass_cursor),
                    _ => *confirm_cursor = cursor_left(confirm, *confirm_cursor),
                }
            }
        }
        Action::CursorRight => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut passphrase,
                ref mut pass_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ..
            }) = state.modal
            {
                match *focused {
                    0 => *pass_cursor = cursor_right(passphrase, *pass_cursor),
                    _ => *confirm_cursor = cursor_right(confirm, *confirm_cursor),
                }
            }
        }
        Action::CursorHome => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut pass_cursor,
                ref mut confirm_cursor,
                ref mut focused,
                ..
            }) = state.modal
            {
                match *focused {
                    0 => *pass_cursor = 0,
                    _ => *confirm_cursor = 0,
                }
            }
        }
        Action::CursorEnd => {
            if let Some(HomeModal::CreateSecretFile {
                ref mut passphrase,
                ref mut pass_cursor,
                ref mut confirm,
                ref mut confirm_cursor,
                ref mut focused,
                ..
            }) = state.modal
            {
                match *focused {
                    0 => *pass_cursor = passphrase.len(),
                    _ => *confirm_cursor = confirm.len(),
                }
            }
        }
        Action::Cancel => {
            state.modal = None;
        }
        Action::Confirm => {
            let (file_path, passphrase, confirm, show) = match &state.modal {
                Some(HomeModal::CreateSecretFile {
                    file_path,
                    passphrase,
                    confirm,
                    show,
                    ..
                }) => (
                    file_path.clone(),
                    passphrase.clone(),
                    confirm.clone(),
                    *show,
                ),
                _ => return Ok(()),
            };

            if passphrase.is_empty() && confirm.is_empty() {
                state.modal = Some(HomeModal::ConfirmNoPassphrase { file_path });
                return Ok(());
            }
            if passphrase != confirm {
                state.modal = Some(HomeModal::CreateSecretFile {
                    file_path,
                    passphrase,
                    pass_cursor: 0,
                    confirm,
                    confirm_cursor: 0,
                    focused: 0,
                    show,
                    error: Some(texts::MSG_PW_MISMATCH.to_string()),
                });
                return Ok(());
            }

            let (outcome, file_path, passphrase) =
                run_with_spinner(terminal, state, Status::Saving, move || {
                    let outcome = file_handling::create_secret_file(&file_path, &passphrase);
                    (outcome, file_path, passphrase)
                })?;
            match outcome {
                Err(e) => {
                    state.modal = Some(HomeModal::CreateSecretFile {
                        file_path,
                        passphrase,
                        pass_cursor: 0,
                        confirm,
                        confirm_cursor: 0,
                        focused: 0,
                        show,
                        error: Some(format!("{e}")),
                    });
                }
                Ok(node) => {
                    state.modal = None;
                    let pp_bytes = zeroize::Zeroizing::new(passphrase.into_bytes());
                    let active_vault = ActiveVaultState {
                        path: Some(file_path.clone()),
                        passphrase: pp_bytes.clone(),
                        node: Some(node.clone()),
                    };
                    state.active_vault = Some(active_vault);

                    let _ = utils::push_recent_secret_file(&mut state.config, &file_path);

                    let mut tree_state = crate::tree::widget::TreeState::new(node);
                    tree_state.display_full_path = state.config.display_full_path;
                    tree_state.generator = state.generator;
                    run_tree(
                        state,
                        terminal,
                        tree_state,
                        Some(SaveTarget {
                            path: file_path,
                            passphrase: pp_bytes,
                        }),
                    )?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

// ── Rendering ──

fn render(frame: &mut Frame, area: Rect, state: &mut HomeState) {
    // Own the background: paint SURFACE across the whole screen first so the
    // terminal's own background never shows through.
    paint_background(frame, area);

    render_home_window(frame, area, state);

    // The file browser renderer maintains its viewport offsets, so it needs
    // mutable access before the remaining modal arms borrow the state.
    if let Some(HomeModal::FileBrowser(browser)) = &mut state.modal {
        file_browser::render_file_browser(frame, area, browser);
        return;
    }

    match &state.modal {
        Some(HomeModal::FileBrowser(_)) => {}
        Some(HomeModal::PassphraseInput { .. }) => render_passphrase_modal(frame, area, state),
        Some(HomeModal::CreateSecretFile { .. }) => {
            render_create_secret_file_modal(frame, area, state)
        }
        Some(HomeModal::Configuration { .. }) => render_config_modal(frame, area, state),
        Some(HomeModal::Alert { message }) => render_alert_modal(frame, area, message),
        Some(HomeModal::DeleteConfirm { path }) => render_delete_confirm_modal(frame, area, path),
        Some(HomeModal::ChangePwConfirm) => render_change_pw_confirm_modal(frame, area),
        Some(HomeModal::ConfirmNoPassphrase { .. }) => {
            render_confirm_no_passphrase_modal(frame, area)
        }
        Some(HomeModal::ChangePwInput { .. }) => render_change_pw_input_modal(frame, area, state),
        Some(HomeModal::ExportSelectType { .. }) => {
            render_export_select_type_modal(frame, area, state)
        }
        Some(HomeModal::ExportConfirm { export_type }) => {
            render_export_confirm_modal(frame, area, export_type)
        }
        None => {}
    }
}

fn render_home_window(frame: &mut Frame, area: Rect, state: &HomeState) {
    let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

    let base_title = state
        .active_vault
        .as_ref()
        .and_then(|af| af.path.as_deref())
        .map(|p| {
            if state.config.display_full_path {
                format!("{} {} ", texts::TITLE_HOME_PREFIX, p)
            } else {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or(std::borrow::Cow::Borrowed(p));
                format!("{} {} ", texts::TITLE_HOME_PREFIX, name)
            }
        })
        .unwrap_or_else(|| texts::TITLE_HOME_DEFAULT.to_string());

    let mut title_spans: Vec<Span> = Vec::new();
    if let Some(status) = state.status {
        let mut text = String::new();
        if status.spins() {
            text.push_str(state.spinner.glyph());
            text.push(' ');
        }
        text.push_str(status.label());
        title_spans.push(Span::styled(format!(" {text}  "), status.style().style()));
    }
    title_spans.push(Span::raw(base_title));

    let block = Block::default()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_style(ElementStyle::HomeBlockBorder.style());

    let inner = block.inner(layout[0]);
    frame.render_widget(block, layout[0]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, item) in texts::MENU_ITEMS.iter().enumerate() {
        let is_selected = state.selected == i;
        let style = if is_selected {
            ElementStyle::HomeSelected.style()
        } else {
            ElementStyle::HomeNormal.style()
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
        ElementStyle::HomeSectionHeader.style(),
    )));
    lines.push(Line::from(""));

    let menu_len = texts::MENU_ITEMS.len();
    for (i, file) in state.config.recent_files.iter().enumerate() {
        let idx = menu_len + i;
        let is_selected = state.selected == idx;
        let style = if is_selected {
            ElementStyle::HomeSelected.style()
        } else {
            ElementStyle::HomeNormal.style()
        };
        let prefix = if is_selected {
            Icon::SelectionArrow.as_str()
        } else {
            Icon::SelectionPad.as_str()
        };
        let display = if state.config.display_full_path {
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

    let (mode_str, mode_style, hint) = home_status_parts(state);
    render_status_bar(frame, layout[1], mode_str, mode_style.style(), &hint);
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

fn render_config_modal(frame: &mut Frame, area: Rect, state: &HomeState) {
    let setting_idx = match &state.modal {
        Some(HomeModal::Configuration { setting_idx }) => *setting_idx,
        _ => 0,
    };

    let overlay_area = make_overlay(area, 10);
    let block = Block::default()
        .title(texts::CONFIG_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeOverlayBlock.style());

    let inner = block.inner(overlay_area);

    let rows = [
        (texts::SETTING_FULL_PATH, state.config.display_full_path),
        (
            texts::SETTING_LAST_FILE,
            state.config.open_last_opened_file_on_startup,
        ),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, (label, value)) in rows.iter().enumerate() {
        let is_selected = i == setting_idx;
        let style = if is_selected {
            ElementStyle::HomeSelected.style()
        } else {
            ElementStyle::HomeNormal.style()
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

    // The colour scheme is a cyclic choice, not an on/off flag, so it shows its
    // current value instead of a checkbox.
    let palette_selected = setting_idx == rows.len();
    let style = if palette_selected {
        ElementStyle::HomeSelected.style()
    } else {
        ElementStyle::HomeNormal.style()
    };
    let prefix = if palette_selected {
        Icon::SelectionArrow.as_str()
    } else {
        Icon::SelectionPad.as_str()
    };
    lines.push(Line::from(Span::styled(
        format!(
            "{}{}: {}",
            prefix,
            texts::SETTING_PALETTE,
            scheme_label(&state.config.palette_name)
        ),
        style,
    )));

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_delete_confirm_modal(frame: &mut Frame, area: Rect, path: &str) {
    let overlay_area = make_overlay(area, 6);
    let block = Block::default()
        .title(texts::HOME_DELETE_CONFIRM)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeDeleteBlock.style());

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
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_change_pw_confirm_modal(frame: &mut Frame, area: Rect) {
    let overlay_area = make_overlay(area, 6);
    let block = Block::default()
        .title(texts::HOME_CHANGE_PASSWORD)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeChangePwBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::PROMPT_CHANGE_PW,
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_confirm_no_passphrase_modal(frame: &mut Frame, area: Rect) {
    let overlay_area = make_overlay(area, 6);
    let block = Block::default()
        .title(texts::HOME_NO_PASSPHRASE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeNoPassBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::PROMPT_NO_PASSPHRASE,
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Builds one password field line. When `show` is true the plaintext is
/// displayed, otherwise the value is masked with `*`. The character under the
/// cursor is inverted so the user can see where edits land.
fn password_field_spans(
    label: &str,
    value: &str,
    cursor: usize,
    focused: bool,
    show: bool,
) -> Vec<Span<'static>> {
    let display: String = if show {
        value.to_string()
    } else {
        "*".repeat(value.len())
    };

    if !focused {
        return vec![
            Span::styled(
                format!("  {}: ", label),
                ElementStyle::HomeFieldLabel.style(),
            ),
            Span::styled(display, ElementStyle::HomeFieldLabel.style()),
        ];
    }

    let pos = cursor.min(display.len());
    let (before, rest) = display.split_at(pos);
    let ch = rest.chars().next();
    let after = &rest[ch.map(|c| c.len_utf8()).unwrap_or(0)..];

    let mut spans = vec![Span::styled(
        format!("  {}: ", label),
        ElementStyle::HomeFieldLabel.style(),
    )];
    if !before.is_empty() {
        spans.push(Span::raw(before.to_string()));
    }
    spans.push(Span::styled(
        ch.map(|c| c.to_string()).unwrap_or_else(|| " ".to_string()),
        ElementStyle::InvertedCursor.style(),
    ));
    if !after.is_empty() {
        spans.push(Span::raw(after.to_string()));
    }
    spans
}

fn render_change_pw_input_modal(frame: &mut Frame, area: Rect, state: &HomeState) {
    let (
        current,
        current_cursor,
        new_pass,
        new_cursor,
        confirm,
        confirm_cursor,
        focused,
        show,
        error,
    ) = match &state.modal {
        Some(HomeModal::ChangePwInput {
            current,
            current_cursor,
            new_pass,
            new_cursor,
            confirm,
            confirm_cursor,
            focused,
            show,
            error,
        }) => (
            current.clone(),
            *current_cursor,
            new_pass.clone(),
            *new_cursor,
            confirm.clone(),
            *confirm_cursor,
            *focused,
            *show,
            error.clone(),
        ),
        _ => return,
    };

    let overlay_area = make_overlay(area, if error.is_some() { 12 } else { 11 });
    let block = Block::default()
        .title(texts::HOME_CHANGE_PASSWORD)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeChangePwBlock.style());

    let inner = block.inner(overlay_area);

    let fields = [
        (texts::LABEL_CURRENT_PW, &current, current_cursor),
        (texts::LABEL_NEW_PW, &new_pass, new_cursor),
        (texts::LABEL_CONFIRM_PW, &confirm, confirm_cursor),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, (label, value, cursor_pos)) in fields.iter().enumerate() {
        lines.push(Line::from(password_field_spans(
            label,
            value,
            *cursor_pos,
            i == focused,
            show,
        )));
    }

    if let Some(ref err_msg) = error {
        lines.push(Line::from(Span::styled(
            format!(" {}", err_msg),
            ElementStyle::HomeErrorBar.style(),
        )));
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_export_select_type_modal(frame: &mut Frame, area: Rect, state: &HomeState) {
    let selected_type = match &state.modal {
        Some(HomeModal::ExportSelectType { selected_type }) => *selected_type,
        _ => 0,
    };

    let overlay_area = make_overlay(area, 9);
    let block = Block::default()
        .title(texts::EXPORT_SELECT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeOverlayBlock.style());

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, item) in texts::EXPORT_TYPE_ITEMS.iter().enumerate() {
        let is_selected = i == selected_type;
        let style = if is_selected {
            ElementStyle::HomeSelected.style()
        } else {
            ElementStyle::HomeNormal.style()
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

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_export_confirm_modal(frame: &mut Frame, area: Rect, export_type: &str) {
    let overlay_area = make_overlay(area, 6);
    let block = Block::default()
        .title(texts::EXPORT_CONFIRM_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeChangePwBlock.style());

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
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_passphrase_modal(frame: &mut Frame, area: Rect, state: &HomeState) {
    let (file_path, passphrase, cursor, show, error) = match &state.modal {
        Some(HomeModal::PassphraseInput {
            file_path,
            passphrase,
            cursor,
            show,
            error,
            ..
        }) => (file_path, passphrase, *cursor, *show, error),
        _ => return,
    };

    let display_path = if state.config.display_full_path {
        file_path.to_string()
    } else {
        std::path::Path::new(file_path.as_str())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string())
    };

    let overlay_area = make_overlay(area, if error.is_some() { 8 } else { 7 });

    let block = Block::default()
        .title(texts::HOME_ENTER_PASSPHRASE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomePassBlock.style());

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{} {}", texts::LABEL_FILE, display_path),
        ElementStyle::CursorLabel.style(),
    )));
    lines.push(Line::from(""));

    let display: String = if show {
        passphrase.clone()
    } else {
        "*".repeat(passphrase.len())
    };
    let before = &display[..cursor.min(display.len())];
    let at = display
        .get(cursor..)
        .and_then(|s: &str| s.chars().next())
        .map(|c: char| c.to_string())
        .unwrap_or_default();
    let after = &display[cursor + at.len()..];

    let cursor_char = if at.is_empty() { " " } else { &at };

    lines.push(Line::from(vec![
        Span::styled(texts::LABEL_PASSPHRASE, ElementStyle::HomePassLabel.style()),
        Span::raw(before.to_string()),
        Span::styled(
            cursor_char.to_string(),
            ElementStyle::InvertedCursor.style(),
        ),
        Span::raw(after.to_string()),
    ]));

    if let Some(err_msg) = error {
        lines.push(Line::from(Span::styled(
            format!(" {}", err_msg),
            ElementStyle::HomeErrorBar.style(),
        )));
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_create_secret_file_modal(frame: &mut Frame, area: Rect, state: &HomeState) {
    let (file_path, passphrase, pass_cursor, confirm, confirm_cursor, focused, show, error) =
        match &state.modal {
            Some(HomeModal::CreateSecretFile {
                file_path,
                passphrase,
                pass_cursor,
                confirm,
                confirm_cursor,
                focused,
                show,
                error,
            }) => (
                file_path,
                passphrase,
                *pass_cursor,
                confirm,
                *confirm_cursor,
                *focused,
                *show,
                error,
            ),
            _ => return,
        };

    let display_path = if state.config.display_full_path {
        file_path.to_string()
    } else {
        std::path::Path::new(file_path.as_str())
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file_path.to_string())
    };

    let overlay_area = make_overlay(area, if error.is_some() { 10 } else { 9 });
    let block = Block::default()
        .title(texts::HOME_CREATE_SECRET)
        .borders(Borders::ALL)
        .style(ElementStyle::HomePassBlock.style());

    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{} {}", texts::LABEL_FILE, display_path),
        ElementStyle::CursorLabel.style(),
    )));
    lines.push(Line::from(""));

    lines.push(Line::from(password_field_spans(
        texts::LABEL_PASSPHRASE_FIELD,
        passphrase,
        pass_cursor,
        focused == 0,
        show,
    )));
    lines.push(Line::from(password_field_spans(
        texts::LABEL_PASSPHRASE_CONFIRM,
        confirm,
        confirm_cursor,
        focused == 1,
        show,
    )));

    if let Some(err_msg) = error {
        lines.push(Line::from(Span::styled(
            format!(" {}", err_msg),
            ElementStyle::HomeErrorBar.style(),
        )));
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_alert_modal(frame: &mut Frame, area: Rect, message: &str) {
    let overlay_area = make_overlay(area, 5);
    let block = Block::default()
        .title(texts::ALERT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeAlertBlock.style());

    let inner = block.inner(overlay_area);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(message, ElementStyle::ConfirmMessage.style())),
        Line::from(""),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).alignment(Alignment::Center),
        inner,
    );
}

/// Maps the active Home modal (or the menu) to the unified status-bar parts:
/// `(mode label, badge style, context hint)`.
///
/// `Alert` has no badge: it is a transient dismissal prompt, not a mode. When
/// the File Browser is open, the badge and hint mirror the browser's own status
/// bar so the two never disagree.
fn home_status_parts(state: &HomeState) -> (&'static str, ElementStyle, Cow<'_, str>) {
    match &state.modal {
        None => (
            texts::MODE_MENU,
            ElementStyle::ModeNormal,
            Cow::Borrowed(texts::HINT_HOME),
        ),
        Some(HomeModal::FileBrowser(browser)) => {
            let (mode_str, mode_style, hint) = file_browser::status_parts(browser);
            (mode_str, mode_style, Cow::Owned(hint))
        }
        Some(HomeModal::PassphraseInput { .. }) => (
            texts::MODE_PASSPHRASE,
            ElementStyle::ModeEdit,
            Cow::Borrowed(texts::HINT_PASSPHRASE),
        ),
        Some(HomeModal::CreateSecretFile { .. }) => (
            texts::MODE_CREATE,
            ElementStyle::ModeEdit,
            Cow::Borrowed(texts::HINT_CREATE_SECRET),
        ),
        Some(HomeModal::Configuration { .. }) => (
            texts::MODE_CONFIG,
            ElementStyle::ModeFieldType,
            Cow::Borrowed(texts::HINT_CONFIG),
        ),
        Some(HomeModal::Alert { .. }) => (
            "",
            ElementStyle::ModeNormal,
            Cow::Borrowed(texts::HINT_ALERT),
        ),
        Some(HomeModal::DeleteConfirm { .. }) => (
            texts::MODE_DELETE,
            ElementStyle::ModeConfirmDelete,
            Cow::Borrowed(texts::HINT_DELETE_CONFIRM),
        ),
        Some(HomeModal::ChangePwConfirm) => (
            texts::MODE_CHANGE_PW,
            ElementStyle::ModeConfirmDelete,
            Cow::Borrowed(texts::HINT_CHANGE_PW_CONFIRM),
        ),
        Some(HomeModal::ChangePwInput { .. }) => (
            texts::MODE_CHANGE_PW,
            ElementStyle::ModeConfirmDelete,
            Cow::Borrowed(texts::HINT_CHANGE_PW_INPUT),
        ),
        Some(HomeModal::ConfirmNoPassphrase { .. }) => (
            texts::MODE_CONFIRM,
            ElementStyle::ModeConfirmDelete,
            Cow::Borrowed(texts::HINT_NO_PASSPHRASE),
        ),
        Some(HomeModal::ExportSelectType { .. }) => (
            texts::MODE_EXPORT,
            ElementStyle::ModeFieldType,
            Cow::Borrowed(texts::HINT_EXPORT_SELECT),
        ),
        Some(HomeModal::ExportConfirm { .. }) => (
            texts::MODE_EXPORT,
            ElementStyle::ModeFieldType,
            Cow::Borrowed(texts::HINT_EXPORT_CONFIRM),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    /// Renders the home screen — including any modal `state` carries — into a
    /// test backend, and asserts that no cell falls through to the terminal's own
    /// background (see [ADR 0018](../../../docs/adr/0018-app-owned-background-fill.md)).
    fn assert_no_terminal_background(state: &mut HomeState) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render(frame, area, state);
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        for (i, cell) in buffer.content.iter().enumerate() {
            assert_ne!(
                cell.style().bg,
                Some(Color::Reset),
                "cell {i} ({:?}) fell through to the terminal background",
                cell.symbol()
            );
        }
    }

    #[test]
    fn home_screen_paints_every_cell() {
        let mut state = HomeState::new();
        assert_no_terminal_background(&mut state);
    }

    /// The File Browser is the largest `Clear` user; the base fill plus its
    /// styled blocks must keep every cell off the terminal background.
    #[test]
    fn file_browser_modal_paints_every_cell() {
        let mut state = HomeState::new();
        let mut browser = file_browser::FileBrowserState::new(
            file_browser::FileBrowserMode::OpenFile,
            &file_browser::FileBrowserSession::default(),
        );
        browser.current_dir = PathBuf::from("/tmp");
        browser.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap();
        state.modal = Some(HomeModal::FileBrowser(browser));

        assert_no_terminal_background(&mut state);
    }
}
