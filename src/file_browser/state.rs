use std::path::PathBuf;

use crate::file_handling::FileEntry;

/// Whether the file browser is used to open an existing file or create a new one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileBrowserMode {
    OpenFile,
    CreateFile,
    ImportFile,
}

/// Filter controlling which entries are shown in the file list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileFilter {
    All,
    PwdKeeperOnly,
}

/// Which part of the browser the user is currently interacting with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusTarget {
    List,
    Input,
    Rename,
}

/// Result returned by the key handler when the user completes or cancels.
#[derive(Debug, Clone, PartialEq)]
pub enum FileBrowserAction {
    /// Full path of the file the user selected (open) or wants to create.
    Selected(String),
    /// User pressed Esc to cancel.
    Cancelled,
    /// A file was renamed (from → to).
    Renamed { from: String, to: String },
}

/// Full state of the file browser modal.
#[derive(Debug, Clone)]
pub struct FileBrowserState {
    pub mode: FileBrowserMode,
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub filter: FileFilter,
    pub search_query: String,
    pub search_active: bool,
    pub creating_name: String,
    pub creating_cursor: usize,
    pub focus: FocusTarget,
    pub error: Option<String>,
    pub rename_name: String,
    pub rename_cursor: usize,
    pub rename_target: String,
    pub confirm_delete: Option<String>,
    /// If set, the file browser will protect this path from deletion
    /// and report renames so the caller can update its state.
    pub active_file_path: Option<PathBuf>,
}

impl FileBrowserState {
    /// Creates a new file browser starting at the user's home directory.
    pub fn new(mode: FileBrowserMode) -> Self {
        let home = dirs_home();
        Self {
            mode,
            current_dir: home.clone(),
            entries: crate::file_handling::list_dir(&home).unwrap_or_default(),
            selected: 0,
            filter: FileFilter::All,
            search_query: String::new(),
            search_active: false,
            creating_name: String::new(),
            creating_cursor: 0,
            focus: FocusTarget::List,
            error: None,
            rename_name: String::new(),
            rename_cursor: 0,
            rename_target: String::new(),
            confirm_delete: None,
            active_file_path: None,
        }
    }

    /// Sets the active file path that the browser should protect.
    pub fn set_active_file_path(&mut self, path: Option<PathBuf>) {
        self.active_file_path = path;
    }

    /// Number of entries after applying the current filter and search query.
    pub fn filtered_len(&self) -> usize {
        self.filtered_indices().len()
    }

    /// Returns indices into `self.entries` that pass the current filter + search.
    pub fn filtered_indices(&self) -> Vec<usize> {
        let query = self.search_query.to_lowercase();
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                let passes_filter = match self.filter {
                    FileFilter::All => true,
                    FileFilter::PwdKeeperOnly => e.is_age_file,
                };
                let passes_search = query.is_empty()
                    || e.name.to_lowercase().contains(&query);
                passes_filter && passes_search
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Reloads entries from the current directory, preserving selection if possible.
    pub fn refresh_entries(&mut self) {
        let selected_name = self.entries.get(self.selected).map(|e| e.name.clone());
        match crate::file_handling::list_dir(&self.current_dir) {
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
        let pos = indices.iter().position(|&i| i >= self.selected).unwrap_or(0);
        let pos = pos.min(indices.len().saturating_sub(1));
        self.selected = indices[pos];
    }
}

/// Returns the user's home directory, or the current directory as fallback.
fn dirs_home() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else if cfg!(target_os = "windows") {
        if let Ok(home) = std::env::var("USERPROFILE") {
            PathBuf::from(home)
        } else {
            PathBuf::from(".")
        }
    } else {
        PathBuf::from(".")
    }
}
