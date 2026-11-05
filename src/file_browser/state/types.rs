//! The small value types the File Browser's state and handlers share.

use std::path::PathBuf;

/// Which part of the browser the user is currently interacting with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusTarget {
    List,
    Input,
    Rename,
    /// The Locations overlay (mounted disks + Home) is open.
    Locations,
}

/// What a click on the File Browser means, recorded by its renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserHit {
    /// A file-list row, by its index in `entries` (before search filtering).
    Entry(usize),
    /// A Locations overlay row, by its index in `locations`.
    Location(usize),
}

/// Transient prefix state for the `a` key in `CreateFile` mode.
///
/// The first `a` arms the prefix; the next key decides the action (`f` for a
/// new file, `d` for a new directory). Any other key cancels. Mirrors the tree's
/// prefix modes (`PendingCut` / `PendingAdd`) without adding an `InputMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddPrefix {
    /// No prefix pending.
    #[default]
    None,
    /// First `a` seen; waiting for the follow-up key.
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
/// disk.  The same slot is used by all [`crate::file_browser::FileBrowserMode`]s.
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
