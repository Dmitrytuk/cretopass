use std::path::{Path, PathBuf};

use crate::file_handling::{self, FileEntry, FileHandlingError};
use crate::theming::texts;

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
    /// The Locations overlay (mounted disks + Home) is open.
    Locations,
}

/// Transient prefix state for the `n` key in `CreateFile` mode.
///
/// The first `n` arms the prefix; the next key decides the action (`n` for a
/// new file, `N` for a new folder). Any other key cancels. Mirrors the tree's
/// prefix modes (`PendingCut` / `PendingAdd`) without adding an `InputMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NewFilePrefix {
    /// No prefix pending.
    #[default]
    None,
    /// First `n` seen; waiting for the follow-up key.
    Pending,
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

/// Session-scoped memory shared by successive file browser openings.
///
/// Lives for the duration of the program run only; it is never written to
/// disk.  The same slot is used by all [`FileBrowserMode`]s.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileBrowserSession {
    /// Last directory the browser successfully displayed.
    pub last_dir: Option<PathBuf>,
    /// Whether hidden (dot-prefixed) entries are shown.
    pub show_hidden: bool,
}

/// A selectable row in the Locations overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// Primary label shown in the row (`Home` or the mount point).
    pub label: String,
    /// Directory the location points at.
    pub path: PathBuf,
    /// Secondary information shown after the label.
    pub detail: String,
    /// Whether this is the pinned Home shortcut.
    pub is_home: bool,
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
    pub active_vault_path: Option<PathBuf>,
    /// Prefix state for the `n` key (new file / new folder) in `CreateFile` mode.
    pub pending: NewFilePrefix,
    /// Whether hidden (dot-prefixed) entries are currently listed.
    pub show_hidden: bool,
    /// Rows of the Locations overlay; empty until it is opened.
    pub locations: Vec<Location>,
    /// Selected row in the Locations overlay.
    pub location_selected: usize,
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
            active_vault_path: None,
            pending: NewFilePrefix::default(),
            show_hidden: session.show_hidden,
            locations: Vec::new(),
            location_selected: 0,
        }
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
                let passes_search = query.is_empty() || e.name.to_lowercase().contains(&query);
                passes_filter && passes_search
            })
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

/// Lists `path` honouring the hidden-entry setting.
pub(super) fn list_entries(
    path: &Path,
    show_hidden: bool,
) -> Result<Vec<FileEntry>, FileHandlingError> {
    if show_hidden {
        file_handling::list_dir_including_hidden(path)
    } else {
        file_handling::list_dir(path)
    }
}

/// Picks the initial directory and its listing for a new browser.
fn initial_location(session: &FileBrowserSession) -> (PathBuf, Vec<FileEntry>) {
    if let Some(dir) = session.last_dir.as_deref()
        && let Ok(entries) = list_entries(dir, session.show_hidden)
    {
        return (dir.to_path_buf(), entries);
    }

    let home = dirs_home();
    let entries = list_entries(&home, session.show_hidden).unwrap_or_default();
    (home, entries)
}

/// Builds the Locations overlay rows: Home first, then mounted disks.
pub(super) fn build_locations() -> Vec<Location> {
    let home = dirs_home();
    let mut locations = Vec::new();
    locations.push(Location {
        label: texts::LOCATION_HOME.to_string(),
        detail: home.to_string_lossy().to_string(),
        path: home,
        is_home: true,
    });

    for disk in file_handling::list_disks() {
        locations.push(Location {
            label: disk.mount_point.to_string_lossy().to_string(),
            detail: format_disk_detail(&disk),
            path: disk.mount_point,
            is_home: false,
        });
    }

    locations
}

/// Selects the most specific location containing `current_dir` (Home wins
/// when the directory is inside the home tree).  Falls back to the first row.
pub(super) fn preselect_location(locations: &[Location], current_dir: &Path) -> usize {
    locations
        .iter()
        .enumerate()
        .filter(|(_, location)| current_dir.starts_with(&location.path))
        .max_by_key(|(_, location)| location.path.components().count())
        .map(|(index, _)| index)
        .unwrap_or(0)
}

/// Formats the secondary information for a disk row.
fn format_disk_detail(disk: &file_handling::DiskInfo) -> String {
    let mut detail = disk.file_system.clone();
    if disk.total_space > 0 {
        detail.push_str(&format!(
            " · {} free / {}",
            format_size(disk.available_space),
            format_size(disk.total_space)
        ));
    }
    if disk.is_removable {
        detail.push(' ');
        detail.push_str(texts::LOCATION_USB);
    }
    if disk.is_read_only {
        detail.push(' ');
        detail.push_str(texts::LOCATION_READ_ONLY);
    }
    detail
}

/// Formats a byte count using binary units and one decimal place.
fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
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

#[cfg(test)]
mod tests {
    use super::*;

    fn disk(mount_point: &str, file_system: &str) -> file_handling::DiskInfo {
        file_handling::DiskInfo {
            mount_point: PathBuf::from(mount_point),
            file_system: file_system.to_string(),
            total_space: 500 * 1024 * 1024 * 1024,
            available_space: 120 * 1024 * 1024 * 1024,
            is_removable: false,
            is_read_only: false,
        }
    }

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

    #[test]
    fn preselect_prefers_most_specific_location() {
        let locations = vec![
            Location {
                label: "/".to_string(),
                path: PathBuf::from("/"),
                detail: String::new(),
                is_home: false,
            },
            Location {
                label: "Home".to_string(),
                path: PathBuf::from("/home/user"),
                detail: String::new(),
                is_home: true,
            },
        ];

        let selected = preselect_location(&locations, Path::new("/home/user/docs"));

        assert_eq!(selected, 1);
    }

    #[test]
    fn preselect_falls_back_to_first_location() {
        let locations = vec![Location {
            label: "Home".to_string(),
            path: PathBuf::from("/home/user"),
            detail: String::new(),
            is_home: true,
        }];

        let selected = preselect_location(&locations, Path::new("/mnt/other"));

        assert_eq!(selected, 0);
    }

    #[test]
    fn build_locations_pins_home_first() {
        let locations = build_locations();

        assert!(locations[0].is_home);
        assert_eq!(locations[0].label, texts::LOCATION_HOME);
    }

    #[test]
    fn format_size_uses_binary_units() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1024 * 1024), "1.0 MiB");
    }

    #[test]
    fn format_disk_detail_includes_tags() {
        let mut entry = disk("/mnt/usb", "vfat");
        entry.is_removable = true;
        entry.is_read_only = true;

        let detail = format_disk_detail(&entry);

        assert!(detail.contains("vfat"));
        assert!(detail.contains(texts::LOCATION_USB));
        assert!(detail.contains(texts::LOCATION_READ_ONLY));
    }

    #[test]
    fn format_disk_detail_omits_unknown_size() {
        let mut entry = disk("/mnt/net", "nfs");
        entry.total_space = 0;
        entry.available_space = 0;

        let detail = format_disk_detail(&entry);

        assert_eq!(detail, "nfs");
    }
}
