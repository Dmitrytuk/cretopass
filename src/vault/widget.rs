//! The Home screen: its event loop, its modal dispatch, and the module tree that
//! holds the rest of it.
//!
//! A key goes to the menu when no modal is open ([`menu`]) and to the active modal
//! otherwise; either way what it asks for is a [`HomeAction`](modal::HomeAction), performed by the
//! single [`orchestrator`]. Modals are pure input, and every screen-level effect
//! lives in the orchestrator.

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::Terminal;
use ratatui::backend::Backend;

mod chrome;
mod menu;
mod modal;
mod orchestrator;
mod render;
mod state;

pub use state::HomeState;

use crate::clipboard;
use crate::theming::SPINNER_TICK;
use crate::theming::Status;
use crate::theming::is_clipboard_paste;

use menu::handle_home_key;
use modal::Modal;
use render::render;

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

        // While the File Browser is waiting for a directory listing, keep
        // redrawing on the spinner tick so the UI never looks frozen — but still
        // read keys, so Esc closes the browser immediately while the fs call is
        // stuck on another thread.
        if browser_is_loading(&state) {
            // Apply any completed listing every iteration, so a result is never
            // stuck behind a stream of keys.
            poll_browser_listing(&mut state);
            if event::poll(SPINNER_TICK)? {
                handle_event(event::read()?, &mut state, terminal)?;
            } else {
                state.spinner.advance();
            }
            continue;
        }

        handle_event(event::read()?, &mut state, terminal)?;
    }
}

/// Handles one terminal event: a `Ctrl+V` is serviced as a clipboard paste, a
/// key press goes to the normal dispatcher, and anything else is ignored.
fn handle_event<B: Backend>(
    event: Event,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if is_clipboard_paste(&key) {
                paste_from_clipboard(state);
            } else {
                dispatch_event(key, state, terminal)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Applies pasted `text` to the active modal's focused input, if it has one.
fn apply_paste(text: &str, state: &mut HomeState) {
    if let Some(modal) = state.modal.as_mut() {
        modal.handle_paste(text);
    }
}

/// Reads the system clipboard and applies it to the active surface.
///
/// An unreadable or empty clipboard is a silent no-op: pasting must never show
/// a false success or disturb the field. With no modal open — the menu itself
/// has no text input — the read is skipped entirely: it is synchronous, and a
/// hung X11 clipboard owner must not freeze the UI for a `Ctrl+V` that provably
/// does nothing.
fn paste_from_clipboard(state: &mut HomeState) {
    if state.modal.is_none() {
        return;
    }
    if let Ok(text) = clipboard::paste_text() {
        apply_paste(&text, state);
    }
}

/// Routes a key to the menu or the active modal.
fn dispatch_event<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    if state.modal.is_none() {
        handle_home_key(key, state, terminal)
    } else {
        handle_modal_key(key, state, terminal)
    }
}

/// Whether the open File Browser has a directory listing in flight.
fn browser_is_loading(state: &HomeState) -> bool {
    matches!(&state.modal, Some(Modal::FileBrowser(browser)) if browser.is_loading())
}

/// Applies a completed File Browser listing, if any.
fn poll_browser_listing(state: &mut HomeState) {
    if let Some(Modal::FileBrowser(browser)) = &mut state.modal {
        browser.poll_listing();
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
    let outcome = run_animated(terminal, state, work);
    // Cleared here rather than inside `run_animated`, so a failed draw or a
    // failed spawn cannot leave the title bar pinned on the busy status.
    state.status = None;
    outcome
}

/// The draw/spin/join loop behind [`run_with_spinner`].
fn run_animated<B, T, F>(
    terminal: &mut Terminal<B>,
    state: &mut HomeState,
    work: F,
) -> io::Result<T>
where
    B: Backend,
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
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

    handle
        .join()
        .map_err(|_| io::Error::other("blocking worker thread panicked"))
}

// ── Modal key handling ──

/// Dispatches a key to the active modal.
///
/// The modal reads the key itself and reports a [`HomeAction`](modal::HomeAction), which
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_browser::{FileBrowserMode, FileBrowserSession};
    use crate::file_handling;
    use crate::theming::{Palette, texts};
    use crate::vault::active_file::ActiveVaultState;
    use crate::vault::utils::Configuration;
    use crate::vault::widget::modal::{About, BrowserModal, ConfigurationModal, ExportWizard};
    use crate::vault::widget::render::home_status_parts;
    use crossterm::event::KeyCode;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use std::path::PathBuf;

    // Every test below builds its `HomeState` through `with_config`, so it never
    // reads the process environment or touches the developer's real config
    // directory. The environment is only ever *read* — nothing in the test suite
    // calls `env::set_var`, which would be undefined behaviour under the default
    // parallel runner.

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
    fn the_home_loop_helpers_poll_a_pending_browser_listing() {
        let mut state = HomeState::with_config(Configuration::default());
        state.modal = Some(Modal::FileBrowser(Box::new(BrowserModal::new(
            FileBrowserMode::OpenFile,
            &FileBrowserSession::default(),
        ))));

        assert!(
            browser_is_loading(&state),
            "a fresh browser waits on its listing"
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while browser_is_loading(&state) && std::time::Instant::now() < deadline {
            poll_browser_listing(&mut state);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }

        assert!(
            !browser_is_loading(&state),
            "the loop helper must apply the completed listing"
        );
    }

    #[test]
    fn home_screen_paints_every_cell() {
        let mut state = HomeState::with_config(Configuration::default());
        assert_no_terminal_background(&mut state);
    }

    /// The File Browser is the largest `Clear` user; the base fill plus its
    /// styled blocks must keep every cell off the terminal background.
    #[test]
    fn file_browser_modal_paints_every_cell() {
        let mut state = HomeState::with_config(Configuration::default());
        let mut browser =
            BrowserModal::new(FileBrowserMode::OpenFile, &FileBrowserSession::default());
        let inner = browser.state_mut();
        inner.current_dir = PathBuf::from("/tmp");
        inner.entries = file_handling::list_dir(std::path::Path::new("/tmp")).unwrap();
        state.modal = Some(Modal::FileBrowser(Box::new(browser)));

        assert_no_terminal_background(&mut state);
    }

    /// The About overlay is also a `Clear` user; the base fill plus its styled
    /// block must keep every cell off the terminal background.
    #[test]
    fn about_modal_paints_every_cell() {
        let mut state = HomeState::with_config(Configuration::default());
        state.modal = Some(Modal::About(About));

        assert_no_terminal_background(&mut state);
    }

    /// The About overlay carries its own `ABOUT` badge and dismiss hint on the
    /// shared status bar, like every other Home mode.
    #[test]
    fn about_modal_status_bar_parts() {
        let mut state = HomeState::with_config(Configuration::default());
        state.modal = Some(Modal::About(About));

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
        let mut state = HomeState::with_config(Configuration::default());
        state.modal = Some(Modal::Configuration(ConfigurationModal::new()));
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

    /// Reopening the dialog takes a fresh revert snapshot: a change made while
    /// the dialog is closed (a file opened in the meantime) must survive the
    /// next cancel, not roll back to the copy taken at startup.
    #[test]
    fn reopening_the_config_dialog_refreshes_the_revert_snapshot() {
        let mut state = HomeState::with_config(Configuration::default());
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let enter = || event::KeyEvent::from(KeyCode::Enter);
        let esc = || event::KeyEvent::from(KeyCode::Esc);

        // Open the dialog from the menu (row 2) and cancel.
        state.selected = 2;
        handle_home_key(enter(), &mut state, &mut terminal).unwrap();
        handle_modal_key(esc(), &mut state, &mut terminal).unwrap();

        // A file opened in the meantime changes the live config.
        state.config.recent_files.push("/opened.age".to_string());

        // Reopen and cancel: the fresh snapshot must win.
        state.selected = 2;
        handle_home_key(enter(), &mut state, &mut terminal).unwrap();
        handle_modal_key(esc(), &mut state, &mut terminal).unwrap();

        assert!(
            state.config.recent_files.iter().any(|p| p == "/opened.age"),
            "a change made between dialogs must survive the next cancel"
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
        let mut state = HomeState::with_config(Configuration::default());
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
        state.modal = Some(Modal::FileBrowser(Box::new(browser)));

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Enter),
            &mut state,
            &mut terminal,
        )
        .unwrap();

        match &state.modal {
            Some(Modal::Alert(alert)) => {
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
            .find(|n| {
                n.name
                    .starts_with(&format!("{}note_", texts::IMPORT_NAME_PREFIX))
            })
            .expect("the imported tree must be wrapped under the open root");
        assert_eq!(wrapper.children.len(), 1);
        assert_eq!(wrapper.children[0].name, "Secrets");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The symmetric failure pin for the `ImportFile` flow: a malformed import
    /// is reported in an alert and changes nothing — no wrapper node, no save.
    #[test]
    fn a_malformed_import_file_is_reported_and_changes_nothing() {
        let dir =
            std::env::temp_dir().join(format!("cretopass-import-bad-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let vault_path = dir.join("vault.age").to_string_lossy().to_string();
        // Well-formed XML around a `<node>` with no `name` attribute.
        std::fs::write(dir.join("bad.xml"), "<node>no name here</node>").unwrap();

        let node = file_handling::create_secret_file(&vault_path, "pw").unwrap();
        let mut state = HomeState::with_config(Configuration::default());
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
                .position(|e| e.name == "bad.xml")
                .expect("the fixture must be listed");
        }
        state.modal = Some(Modal::FileBrowser(Box::new(browser)));

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Enter),
            &mut state,
            &mut terminal,
        )
        .unwrap();

        match &state.modal {
            Some(Modal::Alert(alert)) => {
                assert!(
                    alert.message.contains(texts::MSG_FILE_ERROR),
                    "a malformed import must be reported: {}",
                    alert.message
                );
                assert_ne!(
                    alert.message,
                    texts::MSG_DAMAGED_SECRET,
                    "a bad import file does not mean the open vault is damaged"
                );
            }
            _ => panic!("a failed import must be reported in an alert"),
        }

        // Nothing was written: the vault still holds no wrapper node.
        let data = file_handling::validate_encrypted_file(&vault_path).unwrap();
        let saved = file_handling::decrypt_to_node(&data, "pw").unwrap();
        assert!(
            !saved
                .children
                .iter()
                .any(|n| n.name.starts_with(texts::IMPORT_NAME_PREFIX)),
            "a failed import must not touch the vault"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Home export flow: pick a format, name the destination in the File
    /// Browser, and the plain text lands exactly where it was named.
    #[test]
    fn export_writes_the_plain_text_where_the_browser_named_it() {
        let dir = std::env::temp_dir().join(format!("cretopass-export-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let vault_path = dir.join("vault.age").to_string_lossy().to_string();
        let node = file_handling::create_secret_file(&vault_path, "pw").unwrap();

        let mut state = HomeState::with_config(Configuration::default());
        state.active_vault = Some(ActiveVaultState {
            path: Some(vault_path.clone()),
            passphrase: zeroize::Zeroizing::new(b"pw".to_vec()),
            node: Some(node),
        });
        state.modal = Some(Modal::Export(ExportWizard::new()));

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        // Confirming the format asks for the destination.
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Enter),
            &mut state,
            &mut terminal,
        )
        .unwrap();
        let Some(Modal::FileBrowser(browser)) = &mut state.modal else {
            panic!("the format step must open the File Browser");
        };
        assert_eq!(
            browser.state_mut().creating_name,
            "vault.json",
            "the suggestion comes from the open vault"
        );

        // The suggested name is accepted as it stands and written with `e`.
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Char('e')),
            &mut state,
            &mut terminal,
        )
        .unwrap();

        assert!(
            dir.join("vault.json").exists(),
            "the export must be written where it was named"
        );
        match &state.modal {
            Some(Modal::Alert(alert)) => assert!(
                alert.message.starts_with(texts::MSG_EXPORT_SUCCESS),
                "unexpected alert: {}",
                alert.message
            ),
            _ => panic!("a successful export must be reported in an alert"),
        }
        // The vault itself is untouched.
        assert!(file_handling::is_age_file(std::path::Path::new(
            &vault_path
        )));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Home share flow: name a bundle folder in the File Browser, type the
    /// recipient's password twice, and the encrypted bundle lands where it was
    /// named.
    #[test]
    fn share_writes_an_encrypted_bundle_where_the_browser_named_it() {
        let dir = std::env::temp_dir().join(format!("cretopass-share-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let vault_path = dir.join("vault.age").to_string_lossy().to_string();
        let node = file_handling::create_secret_file(&vault_path, "pw").unwrap();

        let mut state = HomeState::with_config(Configuration::default());
        state.modal = None;
        state.active_vault = Some(ActiveVaultState {
            path: Some(vault_path.clone()),
            passphrase: zeroize::Zeroizing::new(b"pw".to_vec()),
            node: Some(node),
        });

        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        // Menu row 5 is "Share active file".
        state.selected = 5;
        handle_home_key(
            event::KeyEvent::from(KeyCode::Enter),
            &mut state,
            &mut terminal,
        )
        .unwrap();

        let Some(Modal::FileBrowser(browser)) = &mut state.modal else {
            panic!("the share menu item must open the File Browser");
        };
        assert_eq!(
            browser.state_mut().creating_name,
            "vault",
            "the suggestion comes from the open vault"
        );

        // `n` opens the folder input; Enter commits the suggested folder.
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Char('n')),
            &mut state,
            &mut terminal,
        )
        .unwrap();
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Enter),
            &mut state,
            &mut terminal,
        )
        .unwrap();

        // The passphrase step: "pw" twice.
        for c in "pw".chars() {
            handle_modal_key(
                event::KeyEvent::from(KeyCode::Char(c)),
                &mut state,
                &mut terminal,
            )
            .unwrap();
        }
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Tab),
            &mut state,
            &mut terminal,
        )
        .unwrap();
        for c in "pw".chars() {
            handle_modal_key(
                event::KeyEvent::from(KeyCode::Char(c)),
                &mut state,
                &mut terminal,
            )
            .unwrap();
        }
        handle_modal_key(
            event::KeyEvent::from(KeyCode::Enter),
            &mut state,
            &mut terminal,
        )
        .unwrap();

        match &state.modal {
            Some(Modal::Alert(alert)) => assert!(
                alert.message.starts_with(texts::MSG_SHARE_SUCCESS),
                "unexpected alert: {}",
                alert.message
            ),
            _ => panic!("a successful share must be reported in an alert"),
        }

        let bundle = dir.join("vault");
        let file = bundle.join("vault.cretopass");
        assert!(
            file_handling::is_age_file(&file),
            "the bundle file must be encrypted"
        );
        assert!(bundle.join("HOW-TO-OPEN.txt").is_file());
        let data = std::fs::read(&file).unwrap();
        assert!(file_handling::decrypt_to_node(&data, "pw").is_ok());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
