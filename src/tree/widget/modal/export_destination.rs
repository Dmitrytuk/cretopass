//! Naming the destination of a Node export, in the File Browser.
//!
//! The scope and the format are already chosen; this modal only carries them
//! across the browser step, so the confirming selection reports everything as one
//! [`TreeAction::Export`].

use std::path::Path;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::TreeAction;
use crate::file_browser::{self, FileBrowserAction, FileBrowserSession, FileBrowserState};
use crate::file_handling::ExportFormat;
use crate::theming::ElementStyle;
use crate::tree::node::Node;
use crate::tree::widget::TreeState;

/// The File Browser, opened to name an export's destination.
#[derive(Debug)]
pub(crate) struct ExportDestination {
    /// The Node being exported, captured before the browser opened.
    node: Node,
    /// Whether the export includes the Node's children.
    include_children: bool,
    /// The format the export writes.
    format: ExportFormat,
    /// The browser naming the file, prefilled from the secret file and the Node.
    browser: FileBrowserState,
}

impl ExportDestination {
    /// A browser for exporting `node` as `format`.
    ///
    /// `base_path` is the open secret file, when there is one: it seeds the
    /// suggested name and the starting directory.
    pub(crate) fn new(
        node: Node,
        include_children: bool,
        format: ExportFormat,
        base_path: Option<&Path>,
        session: &FileBrowserSession,
    ) -> Self {
        let browser =
            FileBrowserState::for_export(format, base_path, Some(node.name.as_str()), session);
        Self {
            node,
            include_children,
            format,
            browser,
        }
    }

    /// Reads a key: [`TreeAction::Close`] once the browser is cancelled,
    /// [`TreeAction::Export`] when a destination is named.
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        match file_browser::handle_key(key, &mut self.browser) {
            None => None,
            Some(FileBrowserAction::Cancelled) => Some(TreeAction::Close),
            Some(FileBrowserAction::Selected(path)) => Some(TreeAction::Export {
                node: self.node.clone(),
                include_children: self.include_children,
                format: self.format,
                path,
            }),
            // The export mode lists no writable entries, so the browser has no
            // rename to report.
            Some(FileBrowserAction::Renamed { .. }) => None,
        }
    }

    /// The directory and hidden-entry memory for the next browser opening.
    pub(crate) fn session(&self) -> FileBrowserSession {
        self.browser.session()
    }

    /// Draws the browser over `area`. Its renderer keeps its own viewport
    /// offsets, so it needs mutable access.
    pub(crate) fn render(&mut self, frame: &mut Frame, area: Rect) {
        file_browser::render_file_browser(frame, area, &mut self.browser);
    }

    /// Badge and hint for the tree status bar while the browser is open.
    pub(crate) fn status_parts(&self) -> (&'static str, ElementStyle, String) {
        file_browser::status_parts(&self.browser)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    fn export_of(dir: &Path, format: ExportFormat) -> ExportDestination {
        ExportDestination::new(
            Node::new("Login"),
            false,
            format,
            Some(&dir.join("vault.age")),
            &FileBrowserSession::default(),
        )
    }

    #[test]
    fn the_suggested_name_carries_the_node_name() {
        let dir = tempfile::tempdir().unwrap();

        let destination = export_of(dir.path(), ExportFormat::Json);

        assert_eq!(destination.browser.creating_name, "vault_Login.json");
        assert_eq!(destination.browser.current_dir, dir.path());
    }

    #[test]
    fn esc_on_the_list_cancels_the_export() {
        let dir = tempfile::tempdir().unwrap();
        let mut destination = export_of(dir.path(), ExportFormat::Json);
        let mut state = TreeState::new(Node::new("Root"));

        assert!(matches!(
            destination.handle_key(KeyEvent::from(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn pressing_e_reports_the_export_with_the_node() {
        let dir = tempfile::tempdir().unwrap();
        let mut destination = export_of(dir.path(), ExportFormat::Csv);
        let mut state = TreeState::new(Node::new("Root"));

        match destination.handle_key(KeyEvent::from(KeyCode::Char('e')), &mut state) {
            Some(TreeAction::Export {
                node,
                include_children,
                format,
                path,
            }) => {
                assert_eq!(node.name, "Login");
                assert!(!include_children);
                assert_eq!(format, ExportFormat::Csv);
                assert_eq!(Path::new(&path), dir.path().join("vault_Login.csv"));
            }
            _ => panic!("expected an export action"),
        }
    }

    #[test]
    fn an_existing_plain_text_file_asks_before_it_is_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vault_Login.json"), b"{}").unwrap();
        let mut destination = export_of(dir.path(), ExportFormat::Json);
        let mut state = TreeState::new(Node::new("Root"));

        assert!(
            destination
                .handle_key(KeyEvent::from(KeyCode::Char('e')), &mut state)
                .is_none()
        );
        assert!(destination.browser.confirm_overwrite.is_some());

        assert!(matches!(
            destination.handle_key(KeyEvent::from(KeyCode::Char('y')), &mut state),
            Some(TreeAction::Export { path, .. })
                if Path::new(&path) == dir.path().join("vault_Login.json")
        ));
    }
}
