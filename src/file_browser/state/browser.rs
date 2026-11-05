//! The File Browser's whole state and the operations over it.

use std::path::{Path, PathBuf};

use crate::file_handling::FileEntry;
use crate::file_handling::{ExportFormat, output_path, suggest_export_name, suggest_share_name};

use super::locations::{initial_location, list_entries};
use super::mode::FileBrowserMode;
use super::types::{AddPrefix, FileBrowserSession, FocusTarget, Location};

/// Full state of the file browser modal.
#[derive(Debug, Clone)]
pub struct FileBrowserState {
    pub mode: FileBrowserMode,
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub search_query: String,
    pub search_active: bool,
    pub creating_name: String,
    pub creating_cursor: usize,
    /// Whether Enter in the name input should create a directory rather than
    /// hand the chosen path back to the caller (add file). Set by the `ad`
    /// prefix and cleared when the input closes or a directory is created.
    pub creating_is_dir: bool,
    pub focus: FocusTarget,
    pub error: Option<String>,
    pub rename_name: String,
    pub rename_cursor: usize,
    pub rename_target: String,
    pub confirm_delete: Option<String>,
    /// If set, the export destination awaiting an overwrite confirmation. It is
    /// the file the export will write — its extension already resolved.
    pub confirm_overwrite: Option<String>,
    /// If set, the file browser will protect this path from deletion
    /// and report renames so the caller can update its state.
    pub active_vault_path: Option<PathBuf>,
    /// Prefix state for the `a` key (add file / add directory) in `CreateFile` mode.
    pub pending: AddPrefix,
    /// Whether hidden (dot-prefixed) entries are currently listed.
    pub show_hidden: bool,
    /// Rows of the Locations overlay; empty until it is opened.
    pub locations: Vec<Location>,
    /// Selected row in the Locations overlay.
    pub location_selected: usize,
    /// First visible row of the file list; maintained by the renderer.
    pub scroll: usize,
    /// First visible row of the Locations overlay; maintained by the renderer.
    pub location_scroll: usize,
}

impl FileBrowserState {
    /// Creates a new file browser.
    ///
    /// Starts at the session's last directory when it is still listable and
    /// falls back to the user's home directory otherwise.  Hidden entries are
    /// shown when the session says so.
    pub fn new(mode: FileBrowserMode, session: &FileBrowserSession) -> Self {
        let (current_dir, entries) = initial_location(session);
        Self {
            mode,
            current_dir,
            entries,
            selected: 0,
            search_query: String::new(),
            search_active: false,
            creating_name: String::new(),
            creating_cursor: 0,
            creating_is_dir: false,
            focus: FocusTarget::List,
            error: None,
            rename_name: String::new(),
            rename_cursor: 0,
            rename_target: String::new(),
            confirm_delete: None,
            confirm_overwrite: None,
            active_vault_path: None,
            pending: AddPrefix::default(),
            show_hidden: session.show_hidden,
            locations: Vec::new(),
            location_selected: 0,
            scroll: 0,
            location_scroll: 0,
        }
    }

    /// A browser opened to name the destination of an export.
    ///
    /// It starts on the directory list, in the secret file's own directory rather
    /// than wherever the browser was last, with [`suggest_export_name`]
    /// prefilled and shown in the path bar. Navigating picks the destination
    /// directory; `n` edits the name and `e` writes. Cancelling it cancels the
    /// export: there is nothing else the browser is for here.
    pub fn for_export(
        format: ExportFormat,
        base_path: Option<&Path>,
        node_name: Option<&str>,
        session: &FileBrowserSession,
    ) -> Self {
        let mut state = Self::new(FileBrowserMode::ExportFile { format }, session);

        if let Some(dir) = base_path
            .and_then(Path::parent)
            .filter(|dir| !dir.as_os_str().is_empty())
            && let Ok(entries) = list_entries(dir, state.show_hidden)
        {
            state.current_dir = dir.to_path_buf();
            state.entries = entries;
            state.selected = 0;
        }

        let suggested = suggest_export_name(base_path, node_name, format);
        state.creating_cursor = suggested.len();
        state.creating_name = suggested;
        state.focus = FocusTarget::List;
        state
    }

    /// A browser opened to name the folder a share bundle is written into.
    ///
    /// It starts on the directory list, in the secret file's own directory
    /// rather than wherever the browser was last, with [`suggest_share_name`]
    /// (the source stem, plus the Node name for a single branch) prefilled. `n`
    /// edits the folder name and Enter in the input commits it; a folder that
    /// already exists and is not empty asks for confirmation first.
    pub fn for_share(
        base_path: Option<&Path>,
        node_name: Option<&str>,
        session: &FileBrowserSession,
    ) -> Self {
        let mut state = Self::new(FileBrowserMode::ShareDestination, session);

        if let Some(dir) = base_path
            .and_then(Path::parent)
            .filter(|dir| !dir.as_os_str().is_empty())
            && let Ok(entries) = list_entries(dir, state.show_hidden)
        {
            state.current_dir = dir.to_path_buf();
            state.entries = entries;
            state.selected = 0;
        }

        let suggested = suggest_share_name(base_path, node_name);
        state.creating_cursor = suggested.len();
        state.creating_name = suggested;
        state.focus = FocusTarget::List;
        state
    }

    /// The exact file an export would write: the open directory joined with the
    /// typed name, with the format's extension resolved (see
    /// [`output_path`]).
    ///
    /// `None` outside `ExportFile` mode, and while the name is blank — there is
    /// nothing to show or write until the user types one.
    pub fn export_target(&self) -> Option<PathBuf> {
        let format = self.mode.export_format()?;
        let name = self.creating_name.trim();
        if name.is_empty() {
            return None;
        }
        Some(output_path(&self.current_dir.join(name), format))
    }

    /// The folder a share bundle would be written into: the open directory joined
    /// with the typed name.
    ///
    /// `None` outside `ShareDestination` mode and while the name is blank — there
    /// is nothing to show until the user types one.
    pub fn share_target(&self) -> Option<PathBuf> {
        if !self.mode.is_share_destination() {
            return None;
        }
        let name = self.creating_name.trim();
        if name.is_empty() {
            return None;
        }
        Some(self.current_dir.join(name))
    }

    /// Sets the active file path that the browser should protect.
    pub fn set_active_vault_path(&mut self, path: Option<PathBuf>) {
        self.active_vault_path = path;
    }

    /// Captures the session memory for the next browser opening.
    pub fn session(&self) -> FileBrowserSession {
        FileBrowserSession {
            last_dir: Some(self.current_dir.clone()),
            show_hidden: self.show_hidden,
        }
    }

    /// Number of entries after applying the current search query.
    pub fn filtered_len(&self) -> usize {
        self.filtered_indices().len()
    }

    /// Returns indices into `self.entries` that pass the current search.
    pub fn filtered_indices(&self) -> Vec<usize> {
        let query = self.search_query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| query.is_empty() || e.name.to_lowercase().contains(&query))
            .map(|(i, _)| i)
            .collect()
    }

    /// Reloads entries from the current directory, preserving selection if possible.
    pub fn refresh_entries(&mut self) {
        let selected_name = self.entries.get(self.selected).map(|e| e.name.clone());
        let dir = self.current_dir.clone();
        match list_entries(&dir, self.show_hidden) {
            Ok(new_entries) => {
                self.entries = new_entries;
                self.selected = selected_name
                    .and_then(|name| self.entries.iter().position(|e| e.name == name))
                    .unwrap_or(0);
                self.clamp_selection();
            }
            Err(e) => self.error = Some(format!("{e}")),
        }
    }

    /// Clamps `selected` to a valid filtered index.
    pub fn clamp_selection(&mut self) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            self.selected = 0;
            return;
        }
        // Find the position of our current raw selected in the filtered list
        // and clamp to min/max
        let pos = indices
            .iter()
            .position(|&i| i >= self.selected)
            .unwrap_or(0);
        let pos = pos.min(indices.len().saturating_sub(1));
        self.selected = indices[pos];
    }
}

#[cfg(test)]
mod tests {
    use super::super::locations::dirs_home;
    use super::*;

    #[test]
    fn new_uses_session_last_directory() {
        let dir = tempfile::tempdir().unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().to_path_buf()),
            show_hidden: false,
        };

        let state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);

        assert_eq!(state.current_dir, dir.path());
    }

    #[test]
    fn new_falls_back_to_home_for_invalid_last_directory() {
        let dir = tempfile::tempdir().unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().join("missing")),
            show_hidden: false,
        };

        let state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);

        assert_eq!(state.current_dir, dirs_home());
    }

    #[test]
    fn new_lists_hidden_entries_when_session_says_so() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".hidden.txt"), b"hidden").unwrap();
        std::fs::write(dir.path().join("visible.txt"), b"visible").unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().to_path_buf()),
            show_hidden: true,
        };

        let state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);

        assert!(state.entries.iter().any(|e| e.name == ".hidden.txt"));
    }

    #[test]
    fn session_round_trips_directory_and_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let mut state =
            FileBrowserState::new(FileBrowserMode::OpenFile, &FileBrowserSession::default());
        state.current_dir = dir.path().to_path_buf();
        state.show_hidden = true;

        let session = state.session();

        assert_eq!(session.last_dir, Some(dir.path().to_path_buf()));
        assert!(session.show_hidden);
    }
}
