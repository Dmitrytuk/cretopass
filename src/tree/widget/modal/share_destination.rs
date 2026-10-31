//! Naming the folder a share bundle is written into, in the File Browser.
//!
//! The scope is already chosen; this modal carries it (and the selected Node)
//! across the browser step, so the confirming selection reports everything as one
//! [`TreeAction::BeginSharePassphrase`].

use std::path::Path;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;

use super::TreeAction;
use crate::file_browser::{self, FileBrowserAction, FileBrowserSession, FileBrowserState};
use crate::share::ShareScope;
use crate::theming::ElementStyle;
use crate::tree::node::Node;
use crate::tree::widget::TreeState;

/// The File Browser, opened to name a share bundle's folder.
#[derive(Debug)]
pub(crate) struct ShareDestination {
    /// The Node chosen before the browser opened.
    node: Node,
    /// Whether `node` is the branch to share or the whole file is.
    scope: ShareScope,
    /// The browser naming the folder, prefilled from the secret file and the Node.
    browser: FileBrowserState,
}

impl ShareDestination {
    /// A browser for sharing `node` as `scope`.
    ///
    /// `base_path` is the open secret file, when there is one: it seeds the
    /// suggested folder name and the starting directory.
    pub(crate) fn new(
        node: Node,
        scope: ShareScope,
        base_path: Option<&Path>,
        session: &FileBrowserSession,
    ) -> Self {
        let node_name = match scope {
            ShareScope::Branch => Some(node.name.as_str()),
            ShareScope::WholeFile => None,
        };
        let browser = FileBrowserState::for_share(base_path, node_name, session);
        Self {
            node,
            scope,
            browser,
        }
    }

    /// Reads a key: [`TreeAction::Close`] once the browser is cancelled,
    /// [`TreeAction::BeginSharePassphrase`] when a folder is named.
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        _state: &mut TreeState,
    ) -> Option<TreeAction> {
        match file_browser::handle_key(key, &mut self.browser) {
            None => None,
            Some(FileBrowserAction::Cancelled) => Some(TreeAction::Close),
            Some(FileBrowserAction::Selected(dest)) => Some(TreeAction::BeginSharePassphrase {
                node: self.node.clone(),
                scope: self.scope,
                dest,
            }),
            // The share mode lists no writable entries, so the browser has no
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

    fn destination(dir: &Path, scope: ShareScope) -> ShareDestination {
        ShareDestination::new(
            Node::new("Work"),
            scope,
            Some(&dir.join("vault.age")),
            &FileBrowserSession::default(),
        )
    }

    #[test]
    fn a_branch_suggestion_carries_the_node_name() {
        let dir = tempfile::tempdir().unwrap();

        let destination = destination(dir.path(), ShareScope::Branch);

        assert_eq!(destination.browser.creating_name, "vault_Work");
        assert_eq!(destination.browser.current_dir, dir.path());
    }

    #[test]
    fn a_whole_file_suggestion_uses_the_stem_alone() {
        let dir = tempfile::tempdir().unwrap();

        let destination = destination(dir.path(), ShareScope::WholeFile);

        assert_eq!(destination.browser.creating_name, "vault");
    }

    #[test]
    fn esc_on_the_list_cancels_the_share() {
        let dir = tempfile::tempdir().unwrap();
        let mut destination = destination(dir.path(), ShareScope::Branch);
        let mut state = TreeState::new(Node::new("Root"));

        assert!(matches!(
            destination.handle_key(KeyEvent::from(KeyCode::Esc), &mut state),
            Some(TreeAction::Close)
        ));
    }

    #[test]
    fn committing_the_folder_asks_for_the_password() {
        let dir = tempfile::tempdir().unwrap();
        let mut destination = destination(dir.path(), ShareScope::Branch);
        let mut state = TreeState::new(Node::new("Root"));

        // `n` opens the folder input; Enter commits the suggested folder.
        let _ = destination.handle_key(KeyEvent::from(KeyCode::Char('n')), &mut state);
        match destination.handle_key(KeyEvent::from(KeyCode::Enter), &mut state) {
            Some(TreeAction::BeginSharePassphrase { node, scope, dest }) => {
                assert_eq!(node.name, "Work");
                assert_eq!(scope, ShareScope::Branch);
                assert_eq!(Path::new(&dest), dir.path().join("vault_Work"));
            }
            _ => panic!("expected a passphrase request"),
        }
    }
}
