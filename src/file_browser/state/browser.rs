//! The File Browser's whole state and the operations over it.

use std::path::{Path, PathBuf};

use crate::file_handling::{
    DirLoader, DiskInfo, DiskLoader, ExportFormat, FileEntry, ListingOutcome, output_path,
    suggest_export_name, suggest_share_name,
};
use crate::theming::texts;

use super::locations::{dirs_home, disk_locations, home_location, preselect_location};
use super::mode::FileBrowserMode;
use super::types::{AddPrefix, FileBrowserSession, FocusTarget, Location};

/// Full state of the file browser modal.
///
/// Directory listing runs off the UI thread through [`DirLoader`]; the handlers
/// only request a listing, and the event loops poll it. `entries` is empty (and
/// [`FileBrowserState::is_loading`] true) while a request is in flight.
#[derive(Debug)]
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
    /// The background directory lister.
    loader: DirLoader,
    /// The background disk enumerator behind the Locations overlay.
    disks: DiskLoader,
    /// The directory and listing to restore when the pending request fails or
    /// times out. `None` while nothing is pending (or for the first request,
    /// which falls back to Home instead).
    previous: Option<(PathBuf, Vec<FileEntry>)>,
    /// Entry name to reselect when the pending listing lands, for a refresh or
    /// a jump whose target is known by name.
    select_on_load: Option<String>,
    /// A path the user just deleted, awaiting the refreshed listing so the
    /// browser can tell whether the name actually disappeared.
    deleted_path: Option<PathBuf>,
}

impl FileBrowserState {
    /// Creates a new file browser.
    ///
    /// Starts at the session's last directory, falling back to the user's home
    /// directory. The listing is requested in the background; the directory is
    /// empty until it arrives.  Hidden entries are shown when the session says
    /// so.
    pub fn new(mode: FileBrowserMode, session: &FileBrowserSession) -> Self {
        let dir = session.last_dir.clone().unwrap_or_else(dirs_home);
        Self::with_initial_dir(mode, session, dir)
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
        let dir = start_dir(base_path, session);
        let mut state =
            Self::with_initial_dir(FileBrowserMode::ExportFile { format }, session, dir);

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
        let dir = start_dir(base_path, session);
        let mut state = Self::with_initial_dir(FileBrowserMode::ShareDestination, session, dir);

        let suggested = suggest_share_name(base_path, node_name);
        state.creating_cursor = suggested.len();
        state.creating_name = suggested;
        state.focus = FocusTarget::List;
        state
    }

    /// Builds the state at `dir` and requests its listing.
    ///
    /// The request is the "first" one: it has no previous listing to restore, so
    /// a failure falls back to the home directory instead.
    fn with_initial_dir(mode: FileBrowserMode, session: &FileBrowserSession, dir: PathBuf) -> Self {
        let mut state = Self {
            mode,
            current_dir: dir.clone(),
            entries: Vec::new(),
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
            loader: DirLoader::new(),
            disks: DiskLoader::new(),
            previous: None,
            select_on_load: None,
            deleted_path: None,
        };
        state.loader.request(&dir, state.show_hidden);
        state
    }

    /// Opens the Locations overlay.
    ///
    /// The pinned Home row appears at once; mounted disks are enumerated on a
    /// background thread and appended when they arrive, so a disconnected
    /// network or cloud drive cannot stall the overlay.
    pub fn begin_locations(&mut self) {
        self.locations = vec![home_location()];
        self.location_selected = 0;
        self.location_scroll = 0;
        self.disks.request();
    }

    /// Whether the Locations overlay is still waiting for its disks.
    pub fn locations_loading(&self) -> bool {
        self.disks.is_pending()
    }

    /// Appends the enumerated disks and re-runs the row pre-selection.
    fn append_disk_locations(&mut self, disks: Vec<DiskInfo>) {
        self.locations.extend(disk_locations(&disks));
        if self.focus == FocusTarget::Locations {
            self.location_selected = preselect_location(&self.locations, &self.current_dir);
        }
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

    /// Requests the current directory again, keeping the selection by name.
    ///
    /// Used after a delete or rename and when the hidden-entry setting flips.
    pub fn refresh_entries(&mut self) {
        let selected_name = self.entries.get(self.selected).map(|e| e.name.clone());
        let dir = self.current_dir.clone();
        self.begin_listing(dir, selected_name);
    }

    /// Records a path the user just deleted and refreshes the listing.
    ///
    /// When the refreshed listing lands, `reconcile_pending_delete` checks
    /// whether the name survived: a sync client (or the OS) that still
    /// holds the handle keeps it visible — Windows reports the delete as
    /// "pending" — and the browser explains it rather than appearing to ignore
    /// the delete.
    pub fn note_deleted_path(&mut self, path: PathBuf) {
        self.deleted_path = Some(path);
        self.refresh_entries();
    }

    /// Reports a delete that the filesystem has not applied yet.
    ///
    /// A no-op unless a delete is awaiting its listing. Called after a listing
    /// lands.
    pub(crate) fn reconcile_pending_delete(&mut self) {
        let Some(deleted) = self.deleted_path.take() else {
            return;
        };
        if self.entries.iter().any(|entry| entry.path == deleted) {
            self.error = Some(texts::MSG_DELETE_PENDING.to_string());
        }
    }

    /// Navigates to `dir` and requests its listing, clearing any search.
    ///
    /// `select` names the entry to select once the listing lands (the child the
    /// user came from, when going up a level), or `None` to select the first.
    pub fn navigate_to(&mut self, dir: PathBuf, select: Option<String>) {
        self.search_active = false;
        self.search_query.clear();
        self.begin_listing(dir, select);
    }

    /// Starts a listing for `dir`, remembering what to restore on failure.
    ///
    /// When a request is already in flight the last known-good listing is kept:
    /// its `current_dir`/`entries` are not a usable snapshot (the directory has
    /// already been retargeted and the entries cleared), so overwriting
    /// `previous` with them would restore an empty directory. Only a request
    /// started from a settled state refreshes the snapshot.
    fn begin_listing(&mut self, dir: PathBuf, select: Option<String>) {
        self.select_on_load = select;
        if self.loader.is_pending() {
            self.current_dir = dir.clone();
            self.entries.clear();
        } else {
            self.previous = Some((
                std::mem::replace(&mut self.current_dir, dir.clone()),
                std::mem::take(&mut self.entries),
            ));
        }
        self.selected = 0;
        self.loader.request(&dir, self.show_hidden);
    }

    /// Applies a completed listing request, returning `true` when the state
    /// changed (so the event loop redraws).
    ///
    /// A loaded result replaces the listing; a failed or timed-out request
    /// restores the previous directory and shows the reason, or falls back to
    /// the home directory when there is nothing to restore.
    pub fn poll_listing(&mut self) -> bool {
        let mut changed = false;

        if let Some(disks) = self.disks.poll() {
            self.append_disk_locations(disks);
            changed = true;
        }

        if let Some(outcome) = self.loader.poll() {
            match outcome {
                ListingOutcome::Loaded { entries, .. } => {
                    self.previous = None;
                    self.entries = entries;
                    self.selected = self
                        .select_on_load
                        .take()
                        .and_then(|name| self.entries.iter().position(|e| e.name == name))
                        .unwrap_or(0);
                    self.clamp_selection();
                    self.reconcile_pending_delete();
                }
                ListingOutcome::Failed { error, .. } => self.on_listing_failure(format!("{error}")),
                ListingOutcome::TimedOut { .. } => {
                    self.on_listing_failure(texts::MSG_DIRECTORY_NOT_RESPONDING.to_string());
                }
            }
            changed = true;
        }

        changed
    }

    /// Handles a failed or timed-out listing.
    fn on_listing_failure(&mut self, message: String) {
        self.select_on_load = None;

        if let Some((dir, entries)) = self.previous.take() {
            self.current_dir = dir;
            self.entries = entries;
            self.clamp_selection();
            self.error = Some(message);
            return;
        }

        // The first listing failed: fall back to the home directory, as the
        // synchronous constructor used to, without an alarm on the way.
        let home = dirs_home();
        if self.current_dir != home {
            self.begin_listing(home, None);
        } else {
            self.error = Some(message);
        }
    }

    /// Returns `true` while a directory listing or a disk query is outstanding.
    pub fn is_loading(&self) -> bool {
        self.loader.is_pending() || self.disks.is_pending()
    }

    /// Whether a *directory* listing is in flight.
    ///
    /// Distinct from [`Self::is_loading`], which also tracks the Locations
    /// overlay's disk query: the file list's `Loading…` placeholder must not
    /// appear just because the overlay is enumerating disks.
    pub fn dir_loading(&self) -> bool {
        self.loader.is_pending()
    }

    /// Closes the Locations overlay, abandoning a pending disk query.
    pub fn close_locations(&mut self) {
        self.disks.cancel();
        self.focus = FocusTarget::List;
    }

    /// Abandons the outstanding directory listing and disk query without
    /// waiting for either.
    pub fn cancel(&mut self) {
        self.loader.cancel();
        self.disks.cancel();
        self.previous = None;
        self.select_on_load = None;
        self.deleted_path = None;
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

    /// Blocks until the pending listing lands, for tests.
    ///
    /// The event loops poll asynchronously; a unit test has no loop, so this
    /// waits for the result (bounded so a broken loader cannot hang the suite).
    #[cfg(test)]
    pub(crate) fn settle(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while self.is_loading() && std::time::Instant::now() < deadline {
            self.poll_listing();
            if self.is_loading() {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        // Drain a final result in case it landed between the checks.
        let _ = self.poll_listing();
    }
}

/// The directory a destination browser starts in: the secret file's own
/// directory when there is one, otherwise the session's last directory or home.
fn start_dir(base_path: Option<&Path>, session: &FileBrowserSession) -> PathBuf {
    base_path
        .and_then(Path::parent)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(|| session.last_dir.clone())
        .unwrap_or_else(dirs_home)
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

        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);
        state.settle();

        assert_eq!(state.current_dir, dir.path());
        assert!(!state.is_loading());
    }

    #[test]
    fn new_falls_back_to_home_for_invalid_last_directory() {
        let dir = tempfile::tempdir().unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().join("missing")),
            show_hidden: false,
        };

        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);
        state.settle();

        assert_eq!(state.current_dir, dirs_home());
        assert!(
            state.error.is_none(),
            "a silent fallback must not raise the error popup"
        );
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

        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);
        state.settle();

        assert!(state.entries.iter().any(|e| e.name == ".hidden.txt"));
    }

    #[test]
    fn new_is_loading_before_the_listing_lands() {
        let dir = tempfile::tempdir().unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().to_path_buf()),
            show_hidden: false,
        };

        let state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);

        assert!(state.is_loading());
        assert!(state.entries.is_empty());
    }

    #[test]
    fn a_failed_navigation_restores_the_previous_listing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"x").unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().to_path_buf()),
            show_hidden: false,
        };
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);
        state.settle();
        assert!(state.entries.iter().any(|e| e.name == "keep.txt"));

        state.navigate_to(dir.path().join("missing"), None);
        state.settle();

        assert_eq!(state.current_dir, dir.path(), "the old directory returns");
        assert!(
            state.entries.iter().any(|e| e.name == "keep.txt"),
            "the old listing returns"
        );
        assert!(state.error.is_some(), "the failure is reported");
    }

    #[test]
    fn a_second_navigation_before_the_first_lands_keeps_the_last_good_listing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keep.txt"), b"x").unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().to_path_buf()),
            show_hidden: false,
        };
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);
        state.settle();

        // Two navigations before either listing lands, the second to a missing
        // directory: the restore must use the last *known-good* listing, not the
        // empty in-flight one.
        state.navigate_to(PathBuf::from("/tmp"), None);
        state.navigate_to(dir.path().join("missing"), None);
        state.settle();

        assert_eq!(state.current_dir, dir.path());
        assert!(
            state.entries.iter().any(|e| e.name == "keep.txt"),
            "the last good listing must survive two overlapping navigations"
        );
        assert!(state.error.is_some());
    }

    #[test]
    fn a_delete_that_survives_the_listing_reports_it_through_polling() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("held.age");
        std::fs::write(&file, b"x").unwrap();
        let session = FileBrowserSession {
            last_dir: Some(dir.path().to_path_buf()),
            show_hidden: false,
        };
        let mut state = FileBrowserState::new(FileBrowserMode::OpenFile, &session);
        state.settle();

        // The file is *not* removed: it stands in for a sync client that still
        // holds the name, so the refreshed listing still contains it. Driving
        // this through `settle` (which polls) covers the `poll_listing` ->
        // `reconcile_pending_delete` wiring, not just the reconcile method.
        state.note_deleted_path(file.clone());
        state.settle();

        assert_eq!(state.error.as_deref(), Some(texts::MSG_DELETE_PENDING));
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
