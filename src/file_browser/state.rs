use std::path::PathBuf;

/// Whether the file browser is used to open an existing file or create a new one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileBrowserMode {
    OpenFile,
    CreateFile,
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
}

/// A single entry in the file listing.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_age_file: bool,
}

/// Result returned by the key handler when the user completes or cancels.
#[derive(Debug, Clone)]
pub enum FileBrowserAction {
    /// Full path of the file the user selected (open) or wants to create.
    Selected(String),
    /// User pressed Esc to cancel.
    Cancelled,
}

/// Full state of the file browser modal.
#[derive(Debug, Clone)]
pub struct FileBrowserState {
    pub mode: FileBrowserMode,
    pub current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub scroll: usize,
    pub filter: FileFilter,
    pub search_query: String,
    pub search_active: bool,
    pub creating_name: String,
    pub creating_cursor: usize,
    pub focus: FocusTarget,
    pub error: Option<String>,
}

impl FileBrowserState {
    /// Creates a new file browser starting at the user's home directory.
    pub fn new(mode: FileBrowserMode) -> Self {
        let home = dirs_home();
        Self {
            mode,
            current_dir: home.clone(),
            entries: super::fs::list_dir(&home).unwrap_or_default(),
            selected: 0,
            scroll: 0,
            filter: FileFilter::All,
            search_query: String::new(),
            search_active: false,
            creating_name: String::new(),
            creating_cursor: 0,
            focus: FocusTarget::List,
            error: None,
        }
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
