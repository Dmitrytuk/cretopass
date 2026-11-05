//! The File Browser as a Home modal.
//!
//! The browser keeps its own vocabulary ([`FileBrowserAction`]): what a cancelled
//! key, a rename or a selection *means* depends on why it was opened — picking a
//! file to unlock, naming a new one, choosing something to import, or naming an
//! export's destination. Translating that into [`HomeAction`] happens here, so
//! the orchestrator never learns about the browser's internals.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::HomeAction;
use crate::file_browser::{
    self, FileBrowserAction, FileBrowserMode, FileBrowserSession, FileBrowserState,
};
use crate::file_handling::ExportFormat;
use crate::theming::ElementStyle;

/// The directory browser overlay, as one of the Home modals.
pub struct BrowserModal {
    state: FileBrowserState,
}

impl BrowserModal {
    /// A browser opened in `mode`, seeded with the session's memory of the last
    /// directory and hidden-entry choice.
    pub fn new(mode: FileBrowserMode, session: &FileBrowserSession) -> Self {
        Self {
            state: FileBrowserState::new(mode, session),
        }
    }

    /// A browser opened to name the destination of an export, in `format`.
    ///
    /// `base_path` is the secret file being exported and `node_name` the Node,
    /// when only one is; both feed the prefilled name and the starting directory.
    pub fn for_export(
        format: ExportFormat,
        base_path: Option<&Path>,
        node_name: Option<&str>,
        session: &FileBrowserSession,
    ) -> Self {
        Self {
            state: FileBrowserState::for_export(format, base_path, node_name, session),
        }
    }

    /// A browser opened to name the folder a share bundle is written into.
    ///
    /// `base_path` is the open secret file and `node_name` the Node, when only one
    /// branch is shared; both feed the prefilled folder name and the starting
    /// directory.
    pub fn for_share(
        base_path: Option<&Path>,
        node_name: Option<&str>,
        session: &FileBrowserSession,
    ) -> Self {
        Self {
            state: FileBrowserState::for_share(base_path, node_name, session),
        }
    }

    /// Marks the open vault, so the browser refuses to overwrite or delete it.
    pub fn set_active_vault_path(&mut self, path: Option<PathBuf>) {
        self.state.set_active_vault_path(path);
    }

    /// The browser's own state, for tests that need to steer the viewport.
    #[cfg(test)]
    pub fn state_mut(&mut self) -> &mut FileBrowserState {
        &mut self.state
    }

    /// The directory and hidden-entry memory for the next opening.
    pub fn session(&self) -> FileBrowserSession {
        self.state.session()
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match file_browser::handle_key(key, &mut self.state) {
            None => None,
            Some(FileBrowserAction::Cancelled) => {
                // Abandon any in-flight listing explicitly; the modal is dropped
                // next, but this keeps the intent visible.
                self.state.cancel();
                Some(HomeAction::Close)
            }
            Some(FileBrowserAction::Renamed { from, to }) => {
                Some(HomeAction::RenameFile { from, to })
            }
            Some(FileBrowserAction::Selected(path)) => Some(self.selected(path)),
        }
    }

    /// What accepting `path` means, which is why the browser was opened.
    fn selected(&self, path: String) -> HomeAction {
        match self.state.mode {
            FileBrowserMode::ImportFile => HomeAction::ImportFileChosen { import_path: path },
            FileBrowserMode::CreateFile => HomeAction::CreateFileChosen { file_path: path },
            FileBrowserMode::OpenFile => HomeAction::OpenFileChosen { file_path: path },
            FileBrowserMode::ExportFile { format } => HomeAction::Export { path, format },
            FileBrowserMode::ShareDestination => HomeAction::ShareDestinationChosen { dest: path },
        }
    }

    /// Whether a directory listing is in flight.
    pub fn is_loading(&self) -> bool {
        self.state.is_loading()
    }

    /// Applies a completed directory listing, if any.
    pub fn poll_listing(&mut self) -> bool {
        self.state.poll_listing()
    }

    /// Renders the browser. Its renderer keeps its own viewport offsets, so it
    /// needs mutable access.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        file_browser::render_file_browser(frame, area, &mut self.state);
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        let (mode, style, hint) = file_browser::status_parts(&self.state);
        (mode, style, Cow::Owned(hint))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    fn browser(mode: FileBrowserMode) -> BrowserModal {
        BrowserModal::new(mode, &FileBrowserSession::default())
    }

    #[test]
    fn a_selection_is_translated_by_the_mode_it_was_opened_in() {
        assert!(matches!(
            browser(FileBrowserMode::OpenFile).selected("/tmp/v.age".to_string()),
            HomeAction::OpenFileChosen { file_path } if file_path == "/tmp/v.age"
        ));
        assert!(matches!(
            browser(FileBrowserMode::CreateFile).selected("/tmp/new.age".to_string()),
            HomeAction::CreateFileChosen { file_path } if file_path == "/tmp/new.age"
        ));
        assert!(matches!(
            browser(FileBrowserMode::ImportFile).selected("/tmp/x.json".to_string()),
            HomeAction::ImportFileChosen { import_path } if import_path == "/tmp/x.json"
        ));
        assert!(matches!(
            browser(FileBrowserMode::ExportFile {
                format: ExportFormat::Xml
            })
            .selected("/tmp/x.xml".to_string()),
            HomeAction::Export { path, format }
                if path == "/tmp/x.xml" && format == ExportFormat::Xml
        ));
    }

    #[test]
    fn an_export_browser_prefills_the_suggested_name_beside_the_secret_file() {
        let modal = BrowserModal::for_export(
            ExportFormat::Json,
            Some(Path::new("/tmp/vault.age")),
            None,
            &FileBrowserSession::default(),
        );

        assert_eq!(modal.state.creating_name, "vault.json");
        assert_eq!(modal.state.current_dir, Path::new("/tmp"));
    }

    #[test]
    fn an_export_browser_suggests_a_node_name_for_a_single_node() {
        let modal = BrowserModal::for_export(
            ExportFormat::Xml,
            Some(Path::new("/tmp/vault.age")),
            Some("Work Email"),
            &FileBrowserSession::default(),
        );

        assert_eq!(modal.state.creating_name, "vault_Work_Email.xml");
    }

    #[test]
    fn esc_in_the_list_dismisses_the_browser() {
        let mut modal = browser(FileBrowserMode::OpenFile);
        assert!(matches!(
            modal.handle_key(KeyEvent::from(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }
}
