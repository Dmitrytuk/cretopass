//! Committing the typed name as the destination the current mode names.
//!
//! The `e` key (and Enter in the share name input) lands here. An export
//! resolves the format's extension, refuses a destination whose name looks like
//! a vault (by hint — it never reads the target) and asks before overwriting any
//! other existing file; a share names a folder and asks before writing into a
//! non-empty one. Both end
//! in the same [`FileBrowserAction::Selected`], so the browser's callers never
//! learn which flow produced it.

use std::path::Path;

use crate::file_browser::state::{FileBrowserAction, FileBrowserMode, FileBrowserState};
use crate::file_handling;
use crate::file_handling::{ExportFormat, output_path};
use crate::theming::texts;

/// Commits the typed name as the destination the current mode names.
///
/// `None` when the mode names no destination, when the name is blank, or when a
/// confirmation or an error took over — each is reported in the browser's own
/// popups. The single entry point keeps the `e`-key rule in one place: a new
/// destination mode only teaches [`FileBrowserMode::names_destination`] about
/// itself and reaches every key map at once.
pub(super) fn begin_destination(state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    match state.mode {
        FileBrowserMode::ExportFile { format } => begin_export(state, format),
        FileBrowserMode::ShareDestination => begin_share(state),
        _ => None,
    }
}

/// Starts an export under the typed name, into the directory currently open.
///
/// The format's extension replaces the typed one. A destination whose name
/// looks like a vault is refused by hint (never by reading the target, so
/// browsing stays hydration-free), and any other existing target raises the
/// overwrite confirmation first, so a [`FileBrowserAction::Selected`] is
/// reported only once the destination is known to be writable.
fn begin_export(state: &mut FileBrowserState, format: ExportFormat) -> Option<FileBrowserAction> {
    let name = typed_name(state)?;
    let target = output_path(&state.current_dir.join(name), format);
    if file_handling::looks_like_vault(&target) {
        state.error = Some(texts::MSG_CANNOT_OVERWRITE_SECRET.to_string());
        return None;
    }
    if target.exists() {
        state.confirm_overwrite = Some(target.to_string_lossy().into_owned());
        return None;
    }
    Some(selected(&target))
}

/// Commits the typed name as the folder a share bundle is written into.
///
/// A path that already exists as a file is refused — a bundle is a folder — and
/// an existing folder that is not empty asks for confirmation first, so a
/// re-share can update a bundle this flow produced before.
fn begin_share(state: &mut FileBrowserState) -> Option<FileBrowserAction> {
    let name = typed_name(state)?;
    let target = state.current_dir.join(name);

    if !target.exists() {
        return Some(selected(&target));
    }
    if !target.is_dir() {
        state.error = Some(texts::MSG_CANNOT_SHARE_INTO_FILE.to_string());
        return None;
    }

    match file_handling::is_dir_empty(&target) {
        // An existing empty folder is written into without asking.
        Ok(true) => Some(selected(&target)),
        Ok(false) => {
            state.confirm_overwrite = Some(target.to_string_lossy().into_owned());
            None
        }
        Err(e) => {
            state.error = Some(format!("{e}"));
            None
        }
    }
}

/// The trimmed typed name, setting the "name required" error and returning
/// `None` when it is blank.
fn typed_name(state: &mut FileBrowserState) -> Option<String> {
    let name = state.creating_name.trim();
    if name.is_empty() {
        state.error = Some(texts::MSG_NAME_REQUIRED.to_string());
        return None;
    }
    Some(name.to_string())
}

/// The selection action for `target`.
fn selected(target: &Path) -> FileBrowserAction {
    FileBrowserAction::Selected(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crossterm::event::KeyCode;

    use super::super::fixtures::key;
    use crate::file_browser::handle_key;
    use crate::file_browser::state::FocusTarget;
    use crate::file_browser::{FileBrowserAction, FileBrowserSession, FileBrowserState};
    use crate::file_handling;
    use crate::file_handling::ExportFormat;
    use crate::theming::texts;

    // ── Export destinations ──

    /// An export browser over `dir`, as if opened for the secret file
    /// `dir/vault.age`, in `format`.
    fn export_state(dir: &Path, format: ExportFormat) -> FileBrowserState {
        FileBrowserState::for_export(
            format,
            Some(&dir.join("vault.age")),
            None,
            &FileBrowserSession::default(),
        )
    }

    #[test]
    fn test_export_opens_on_the_list_with_the_suggested_name() {
        let dir = tempfile::tempdir().unwrap();

        let state = export_state(dir.path(), ExportFormat::Json);

        assert_eq!(state.focus, FocusTarget::List);
        assert_eq!(state.creating_name, "vault.json");
        assert_eq!(state.current_dir, dir.path());
    }

    #[test]
    fn test_export_target_resolves_the_format_extension() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Csv);
        state.creating_name = "backup.txt".to_string();

        assert_eq!(state.export_target(), Some(dir.path().join("backup.csv")));
    }

    #[test]
    fn test_export_target_is_none_for_a_blank_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Json);
        state.creating_name = "   ".to_string();

        assert_eq!(state.export_target(), None);
    }

    #[test]
    fn test_export_writes_with_the_format_extension() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Csv);
        state.creating_name = "backup.txt".to_string();

        let result = handle_key(key(KeyCode::Char('e')), &mut state);

        assert_eq!(
            result,
            Some(FileBrowserAction::Selected(
                dir.path().join("backup.csv").to_string_lossy().to_string()
            ))
        );
    }

    #[test]
    fn test_export_on_an_existing_file_asks_before_overwriting() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vault.csv"), b"old").unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Csv);

        let result = handle_key(key(KeyCode::Char('e')), &mut state);

        assert_eq!(result, None, "the overwrite must be confirmed first");
        assert_eq!(
            state.confirm_overwrite.as_deref(),
            Some(dir.path().join("vault.csv").to_string_lossy().as_ref())
        );
    }

    #[test]
    fn test_export_any_key_other_than_yes_cancels_the_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vault.csv"), b"old").unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Csv);
        handle_key(key(KeyCode::Char('e')), &mut state);

        let result = handle_key(key(KeyCode::Char('n')), &mut state);

        assert_eq!(result, None);
        assert!(state.confirm_overwrite.is_none());
    }

    #[test]
    fn test_export_confirming_the_overwrite_selects_the_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vault.csv"), b"old").unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Csv);
        handle_key(key(KeyCode::Char('e')), &mut state);

        let result = handle_key(key(KeyCode::Char('y')), &mut state);

        assert_eq!(
            result,
            Some(FileBrowserAction::Selected(
                dir.path().join("vault.csv").to_string_lossy().to_string()
            ))
        );
    }

    #[test]
    fn test_export_asks_before_overwriting_a_secret_file() {
        let dir = tempfile::tempdir().unwrap();
        // The browser no longer reads the target to recognise a secret file, so
        // a secret carrying the export's own resolved name (`.json`) is handled
        // by the ordinary overwrite confirmation rather than refused outright.
        let secret = dir.path().join("vault.json");
        file_handling::create_secret_file(&secret, "pw").unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Json);

        let result = handle_key(key(KeyCode::Char('e')), &mut state);

        assert_eq!(result, None, "the overwrite must be confirmed first");
        assert!(state.error.is_none());
        assert_eq!(
            state.confirm_overwrite.as_deref(),
            Some(secret.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn test_export_mode_offers_no_rename_or_delete() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("other.age");
        file_handling::create_secret_file(&secret, "pw").unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Json);
        state.focus = FocusTarget::List;
        state.selected = state
            .entries
            .iter()
            .position(|e| e.name == "other.age")
            .expect("the secret file is listed");

        handle_key(key(KeyCode::Char('r')), &mut state);
        handle_key(key(KeyCode::Char('d')), &mut state);

        assert_eq!(state.focus, FocusTarget::List);
        assert!(state.rename_target.is_empty());
        assert!(state.confirm_delete.is_none());
    }

    #[test]
    fn test_enter_on_a_file_row_does_nothing_in_export_mode() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"x").unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Json);
        state.selected = state
            .entries
            .iter()
            .position(|e| e.name == "notes.txt")
            .expect("the file is listed");

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::List, "Enter stays on the list");
        assert_eq!(state.creating_name, "vault.json", "the draft is kept");
    }

    #[test]
    fn test_n_opens_the_export_name_input() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Json);

        let result = handle_key(key(KeyCode::Char('n')), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.focus, FocusTarget::Input);
    }

    #[test]
    fn test_enter_in_the_export_name_input_returns_to_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Json);
        // `n` opens the draft; Enter accepts it without writing.
        handle_key(key(KeyCode::Char('n')), &mut state);
        state.creating_name = "renamed.json".to_string();

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None, "Enter must not start the export");
        assert_eq!(state.focus, FocusTarget::List);
        assert_eq!(state.creating_name, "renamed.json");
    }

    #[test]
    fn test_e_with_a_blank_name_shows_an_error_and_stays_open() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = export_state(dir.path(), ExportFormat::Json);
        state.creating_name = "   ".to_string();

        let result = handle_key(key(KeyCode::Char('e')), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.error.as_deref(), Some(texts::MSG_NAME_REQUIRED));
        assert!(state.confirm_overwrite.is_none());
    }

    // ── Share destinations ──

    /// A share browser over `dir`, as if opened for the secret file
    /// `dir/vault.age`.
    fn share_state(dir: &Path) -> FileBrowserState {
        FileBrowserState::for_share(
            Some(&dir.join("vault.age")),
            None,
            &FileBrowserSession::default(),
        )
    }

    #[test]
    fn test_share_opens_on_the_list_with_the_suggested_folder() {
        let dir = tempfile::tempdir().unwrap();

        let state = share_state(dir.path());

        assert_eq!(state.focus, FocusTarget::List);
        assert_eq!(state.creating_name, "vault");
        assert_eq!(state.current_dir, dir.path());
    }

    #[test]
    fn test_share_enter_commits_a_new_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = share_state(dir.path());
        state.focus = FocusTarget::Input;

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(
            result,
            Some(FileBrowserAction::Selected(
                dir.path().join("vault").to_string_lossy().to_string()
            ))
        );
    }

    #[test]
    fn test_share_enter_refuses_a_path_that_is_a_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("vault"), b"x").unwrap();
        let mut state = share_state(dir.path());
        state.focus = FocusTarget::Input;

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None);
        assert_eq!(
            state.error.as_deref(),
            Some(texts::MSG_CANNOT_SHARE_INTO_FILE)
        );
    }

    #[test]
    fn test_share_enter_writes_into_an_existing_empty_folder_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("vault")).unwrap();
        let mut state = share_state(dir.path());
        state.focus = FocusTarget::Input;

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert!(matches!(result, Some(FileBrowserAction::Selected(_))));
        assert!(state.confirm_overwrite.is_none());
    }

    #[test]
    fn test_share_asks_before_writing_into_a_non_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("vault");
        std::fs::create_dir(&bundle).unwrap();
        std::fs::write(bundle.join("HOW-TO-OPEN.txt"), b"old").unwrap();
        let mut state = share_state(dir.path());
        state.focus = FocusTarget::Input;

        let result = handle_key(key(KeyCode::Enter), &mut state);
        assert_eq!(result, None, "the folder must be confirmed first");
        assert!(state.confirm_overwrite.is_some());

        let confirmed = handle_key(key(KeyCode::Char('y')), &mut state);
        assert!(matches!(confirmed, Some(FileBrowserAction::Selected(_))));
    }

    #[test]
    fn test_share_enter_with_a_blank_name_shows_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = share_state(dir.path());
        state.creating_name = "   ".to_string();
        state.focus = FocusTarget::Input;

        let result = handle_key(key(KeyCode::Enter), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.error.as_deref(), Some(texts::MSG_NAME_REQUIRED));
    }

    #[test]
    fn test_share_mode_offers_no_rename_or_delete() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("other.age");
        file_handling::create_secret_file(&secret, "pw").unwrap();
        let mut state = share_state(dir.path());
        state.selected = state
            .entries
            .iter()
            .position(|e| e.name == "other.age")
            .expect("the secret file is listed");

        handle_key(key(KeyCode::Char('r')), &mut state);
        handle_key(key(KeyCode::Char('d')), &mut state);

        assert_eq!(state.focus, FocusTarget::List);
        assert!(state.rename_target.is_empty());
        assert!(state.confirm_delete.is_none());
    }

    #[test]
    fn test_share_e_commits_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = share_state(dir.path());

        // `e` commits the folder, matching an export's write key.
        let result = handle_key(key(KeyCode::Char('e')), &mut state);

        assert_eq!(
            result,
            Some(FileBrowserAction::Selected(
                dir.path().join("vault").to_string_lossy().to_string()
            ))
        );
    }

    #[test]
    fn test_share_e_with_a_blank_name_shows_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = share_state(dir.path());
        state.creating_name = "   ".to_string();

        let result = handle_key(key(KeyCode::Char('e')), &mut state);

        assert_eq!(result, None);
        assert_eq!(state.error.as_deref(), Some(texts::MSG_NAME_REQUIRED));
    }

    #[test]
    fn test_share_e_asks_before_writing_into_a_non_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("vault");
        std::fs::create_dir(&bundle).unwrap();
        std::fs::write(bundle.join("HOW-TO-OPEN.txt"), b"old").unwrap();
        let mut state = share_state(dir.path());

        let result = handle_key(key(KeyCode::Char('e')), &mut state);
        assert_eq!(result, None, "the folder must be confirmed first");
        assert!(state.confirm_overwrite.is_some());

        let confirmed = handle_key(key(KeyCode::Char('y')), &mut state);
        assert!(matches!(confirmed, Some(FileBrowserAction::Selected(_))));
    }
}
