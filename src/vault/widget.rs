use std::borrow::Cow;
use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal, backend::Backend};

mod chrome;
mod modal;
mod orchestrator;
mod secret_field;

use crate::file_browser::FileBrowserSession;
use crate::file_handling;
use crate::generator::PasswordParams;
use crate::theming::Action;
use crate::theming::{
    ElementStyle, Icon, Palette, SPINNER_TICK, Spinner, Status, paint_background,
    render_status_bar, set_palette, texts,
};
use crate::vault::active_file::ActiveVaultState;
use crate::vault::utils::{self, Configuration};

use modal::{
    About, Alert, BrowserModal, ChangePasswordPrompt, ConfigurationModal, Confirmation,
    CreateSecretPrompt, ExportWizard, HomeAction, UnlockPrompt,
};

enum HomeModal {
    /// The directory browser, which picks a file to unlock, name, or import.
    FileBrowser(BrowserModal),
    /// Unlock prompt: one masked field for the master password.
    Unlock(UnlockPrompt),
    /// Create a new secret file: the master password and its confirmation.
    CreateSecret(CreateSecretPrompt),
    /// The persisted Home preferences.
    Configuration(ConfigurationModal),
    /// A passive message the user acknowledges with any key.
    Alert(Alert),
    /// A yes/no confirmation; the prompt and its consequences follow from the
    /// kind it carries.
    Confirmation(Confirmation),
    /// Change master password: current, new, and confirmation.
    ChangePassword(ChangePasswordPrompt),
    /// The two-step export flow: pick a format, then confirm it.
    Export(ExportWizard),
    /// App identity, version, and feedback links, opened with `?` from the menu.
    About(About),
}

impl HomeModal {
    /// A passive alert carrying `message`.
    fn alert(message: impl Into<String>) -> Self {
        Self::Alert(Alert {
            message: message.into(),
        })
    }

    /// Reads a key: `None` means the modal consumed it and stays open, `Some` is
    /// what the user asked for.
    fn handle_key(&mut self, key: event::KeyEvent) -> Option<HomeAction> {
        match self {
            Self::Alert(alert) => alert.handle_key(key),
            Self::About(about) => about.handle_key(key),
            Self::Unlock(prompt) => prompt.handle_key(key),
            Self::CreateSecret(prompt) => prompt.handle_key(key),
            Self::ChangePassword(prompt) => prompt.handle_key(key),
            Self::Configuration(modal) => modal.handle_key(key),
            Self::Confirmation(confirmation) => confirmation.handle_key(key),
            Self::Export(wizard) => wizard.handle_key(key),
            Self::FileBrowser(browser) => browser.handle_key(key),
        }
    }
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
                    .map(|data| HomeModal::Unlock(UnlockPrompt::new(path.clone(), data)))
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

            if state.modal.is_none() {
                handle_home_key(key, &mut state, terminal)?;
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
/// once the wait ends. If a draw or a poll fails mid-wait the worker keeps
/// running: the save/decrypt is *not* cancelled.
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
        if event::poll(SPINNER_TICK).unwrap_or(false) {
            // Drain every pending event so none is reinterpreted afterwards. An
            // unreadable input source (e.g. stdin is not a terminal) is not fatal
            // here: there is nothing to drain, and the save/decrypt in flight must
            // not be cancelled — same rule as a failed draw above.
            while event::poll(Duration::ZERO).unwrap_or(false) {
                let _ = event::read();
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

/// Handles a key on the Home menu.
///
/// Moving the cursor is pure screen state and handled here; everything else —
/// quitting, opening a modal, launching the Tree, touching the recent list — is
/// reported as a [`HomeAction`] and performed by the orchestrator, exactly as the
/// modals do.
fn handle_home_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let action = Action::from_home(&key);
    match &action {
        Action::NavDown => state.select_next(),
        Action::NavUp => state.select_prev(),
        _ => {
            if let Some(intent) = menu_intent(state, &action) {
                return orchestrator::execute(state, terminal, intent, None);
            }
        }
    }
    Ok(())
}

/// What the Home menu asks for, given the highlighted row.
///
/// `None` means the key needs nothing from the orchestrator: it either moved the
/// cursor or is not bound in this menu.
fn menu_intent(state: &HomeState, action: &Action) -> Option<HomeAction> {
    match action {
        Action::Quit => Some(HomeAction::Quit),
        Action::ReturnToMenu => Some(HomeAction::LaunchTree),
        Action::Back => Some(HomeAction::Back),
        Action::OpenHelp => Some(HomeAction::OpenAbout),
        Action::Confirm => match state.selected {
            0 => Some(HomeAction::ChooseFileToOpen),
            1 => Some(HomeAction::ChooseFileToCreate),
            2 => Some(HomeAction::OpenConfiguration),
            3 => Some(HomeAction::BeginChangePassword),
            4 => Some(HomeAction::BeginExport),
            5 => Some(HomeAction::ChooseFileToImport),
            idx => state
                .config
                .recent_files
                .get(idx - texts::MENU_ITEMS.len())
                .map(|path| HomeAction::OpenRecent { path: path.clone() }),
        },
        Action::ConfigDelete => state
            .selected
            .checked_sub(texts::MENU_ITEMS.len())
            .and_then(|idx| state.config.recent_files.get(idx))
            .map(|path| HomeAction::RequestDeleteRecent { path: path.clone() }),
        _ => None,
    }
}

// ── Modal key handling ──

/// Dispatches a key to the active modal.
///
/// The modal reads the key itself and reports a [`HomeAction`], which
/// [`orchestrator::execute`] then performs. The File Browser never reaches here:
/// it owns its own viewport state and is dispatched by the event loop.
fn handle_modal_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let Some(mut modal) = state.modal.take() else {
        return Ok(());
    };

    match modal.handle_key(key) {
        // The modal consumed the key and stays open.
        None => {
            state.modal = Some(modal);
            Ok(())
        }
        Some(action) => orchestrator::execute(state, terminal, action, Some(modal)),
    }
}

// ── Rendering ──

fn render(frame: &mut Frame, area: Rect, state: &mut HomeState) {
    // Own the background: paint SURFACE across the whole screen first so the
    // terminal's own background never shows through.
    paint_background(frame, area);

    render_home_window(frame, area, state);

    // The File Browser renderer keeps its own viewport offsets, so it needs
    // mutable access before the remaining modal arms borrow the state.
    if let Some(HomeModal::FileBrowser(browser)) = &mut state.modal {
        browser.render(frame, area);
        return;
    }

    match &state.modal {
        Some(HomeModal::FileBrowser(_)) => {}
        Some(HomeModal::Alert(alert)) => alert.render(frame, area),
        Some(HomeModal::About(about)) => about.render(frame, area),
        Some(HomeModal::Confirmation(confirmation)) => confirmation.render(frame, area),
        Some(HomeModal::Unlock(prompt)) => {
            prompt.render(frame, area, state.config.display_full_path)
        }
        Some(HomeModal::CreateSecret(prompt)) => {
            prompt.render(frame, area, state.config.display_full_path)
        }
        Some(HomeModal::Configuration(modal)) => modal.render(frame, area, &state.config),
        Some(HomeModal::ChangePassword(prompt)) => prompt.render(frame, area),
        Some(HomeModal::Export(wizard)) => wizard.render(frame, area),
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
        Some(HomeModal::FileBrowser(browser)) => browser.status_parts(),
        Some(HomeModal::Unlock(prompt)) => prompt.status_parts(),
        Some(HomeModal::CreateSecret(prompt)) => prompt.status_parts(),
        Some(HomeModal::Configuration(modal)) => modal.status_parts(),
        Some(HomeModal::Alert(alert)) => alert.status_parts(),
        Some(HomeModal::Confirmation(confirmation)) => confirmation.status_parts(),
        Some(HomeModal::ChangePassword(prompt)) => prompt.status_parts(),
        Some(HomeModal::Export(wizard)) => wizard.status_parts(),
        Some(HomeModal::About(about)) => about.status_parts(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_browser::FileBrowserMode;
    use crossterm::event::KeyCode;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use std::path::PathBuf;

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
        let mut browser =
            BrowserModal::new(FileBrowserMode::OpenFile, &FileBrowserSession::default());
        let inner = browser.state_mut();
        inner.current_dir = PathBuf::from("/tmp");
        inner.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap();
        state.modal = Some(HomeModal::FileBrowser(browser));

        assert_no_terminal_background(&mut state);
    }

    /// The About overlay is also a `Clear` user; the base fill plus its styled
    /// block must keep every cell off the terminal background.
    #[test]
    fn about_modal_paints_every_cell() {
        let mut state = HomeState::new();
        state.modal = Some(HomeModal::About(About));

        assert_no_terminal_background(&mut state);
    }

    /// The About overlay carries its own `ABOUT` badge and dismiss hint on the
    /// shared status bar, like every other Home mode.
    #[test]
    fn about_modal_status_bar_parts() {
        let mut state = HomeState::new();
        state.modal = Some(HomeModal::About(About));

        let (mode, _, hint) = home_status_parts(&state);
        assert_eq!(mode, texts::MODE_ABOUT);
        assert_eq!(hint.as_ref(), texts::HINT_HELP);
    }

    /// `Esc` in the Configuration modal reverts the live-applied colour scheme
    /// together with the config ([ADR 0019](../../../docs/adr/0019-runtime-colour-scheme-switching.md)).
    /// Guards against a cancel that reloads the config but leaves the swapped
    /// palette on screen until restart.
    #[test]
    fn config_cancel_reverts_the_live_colour_scheme() {
        let mut state = HomeState::new();
        state.modal = Some(HomeModal::Configuration(ConfigurationModal::new()));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

        // Move to the colour-scheme row, then `Space` applies the other scheme at
        // once — through the modal, the action it reports and the orchestrator.
        for _ in 0..2 {
            handle_modal_key(
                event::KeyEvent::from(KeyCode::Char('j')),
                &mut state,
                &mut terminal,
            )
            .unwrap();
        }
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Char(' ')),
            &mut state,
            &mut terminal,
        )
        .unwrap();
        assert_eq!(
            crate::theming::palette(),
            Palette::from_name(&state.config.palette_name),
            "toggling must apply the new scheme live"
        );

        // `Esc` must leave the active scheme matching the reverted config.
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Esc),
            &mut state,
            &mut terminal,
        )
        .unwrap();
        assert_eq!(
            crate::theming::palette(),
            Palette::from_name(&state.config.palette_name),
            "cancelling must revert the colours along with the config"
        );
    }

    /// Minimal importable fixture: one root node with one child.
    const IMPORT_FIXTURE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<node name="Secrets" description="My vault">
  <extra_fields>
    <field type="Note" value="top level note"/>
  </extra_fields>
  <children>
    <node name="Login" description="My email login">
      <extra_fields>
        <field type="Password" value="s3cret"/>
      </extra_fields>
    </node>
  </children>
</node>"#;

    /// Pins the `ImportFile` flow before it moves out of the file-browser handler
    /// into the orchestrator (plan task 9): selecting an importable file parses
    /// it, wraps it in a timestamped node under the open Root, saves the vault,
    /// and reports success in an alert.
    #[test]
    fn import_file_selected_wraps_and_saves_under_the_root() {
        let dir = std::env::temp_dir().join(format!("cretopass-import-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let vault_path = dir.join("vault.age").to_string_lossy().to_string();
        std::fs::write(dir.join("note.xml"), IMPORT_FIXTURE_XML).unwrap();

        let node = file_handling::create_secret_file(&vault_path, "pw").unwrap();
        let mut state = HomeState::new();
        state.active_vault = Some(ActiveVaultState {
            path: Some(vault_path.clone()),
            passphrase: zeroize::Zeroizing::new(b"pw".to_vec()),
            node: Some(node),
        });

        let mut browser =
            BrowserModal::new(FileBrowserMode::ImportFile, &FileBrowserSession::default());
        {
            let inner = browser.state_mut();
            inner.current_dir = dir.clone();
            inner.entries = file_handling::list_dir(&dir).unwrap();
            inner.selected = inner
                .entries
                .iter()
                .position(|e| e.name == "note.xml")
                .expect("the fixture must be listed");
        }
        state.modal = Some(HomeModal::FileBrowser(browser));

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Enter),
            &mut state,
            &mut terminal,
        )
        .unwrap();

        match &state.modal {
            Some(HomeModal::Alert(alert)) => {
                assert_eq!(alert.message, texts::MSG_IMPORT_SUCCESS)
            }
            _ => panic!("a successful import must be reported in an alert"),
        }

        // The import was persisted: re-reading the vault shows the wrapper.
        let data = file_handling::validate_encrypted_file(&vault_path).unwrap();
        let saved = file_handling::decrypt_to_node(&data, "pw").unwrap();
        let wrapper = saved
            .children
            .iter()
            .find(|n| n.name.starts_with("imported_note_"))
            .expect("the imported tree must be wrapped under the open root");
        assert_eq!(wrapper.children.len(), 1);
        assert_eq!(wrapper.children[0].name, "Secrets");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
