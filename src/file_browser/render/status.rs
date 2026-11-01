//! The File Browser's contribution to the unified status bar: its mode badge
//! and the context hint, rendered by the parent window.

use crate::file_browser::state::{
    AddPrefix, FileBrowserMode, FileBrowserState, FileFilter, FocusTarget,
};
use crate::theming::{ElementStyle, texts};

/// Maps the browser's mode and active overlay to the unified status-bar parts:
/// `(mode label, badge style, context hint)`.
///
/// The File Browser does not draw a status bar of its own; the parent window
/// (Home) calls this to render the browser's badge and hint in its status line.
///
/// Transient overlays (error, delete confirmation, Locations picker, rename /
/// name input) capture keys, so they own the hint while shown. The list hint is
/// otherwise dynamic: while searching it is the query prompt, while the `a`
/// prefix is armed it is the add-file prompt, and otherwise it is the set of
/// advertised keys.
///
/// The badge is the browser mode (`OPEN`/`CREATE`/`IMPORT`/`EXPORT`) unless the
/// PwdKeeper-only filter is active, in which case it is `PWD` in every mode.
pub(crate) fn status_parts(state: &FileBrowserState) -> (&'static str, ElementStyle, String) {
    // Overlays are drawn on top of the list and route keys themselves.
    if state.error.is_some() {
        return ("", ElementStyle::ModeNormal, texts::HINT_ALERT.to_string());
    }
    if state.confirm_delete.is_some() {
        return (
            texts::MODE_DELETE,
            ElementStyle::ModeConfirmDelete,
            texts::FB_HINT_DELETE_CONFIRM.to_string(),
        );
    }
    if state.confirm_overwrite.is_some() {
        return (
            texts::MODE_CONFIRM,
            ElementStyle::ModeConfirmDelete,
            texts::FB_HINT_OVERWRITE_CONFIRM.to_string(),
        );
    }
    if state.focus == FocusTarget::Locations {
        return (
            texts::MODE_MENU,
            ElementStyle::ModeNormal,
            texts::FB_HINT_LOCATIONS.to_string(),
        );
    }
    if state.focus == FocusTarget::Rename {
        return (
            texts::MODE_EDIT,
            ElementStyle::ModeEdit,
            fb_input_hint(state),
        );
    }

    let (mode_str, mode_style) = match state.mode {
        FileBrowserMode::OpenFile => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_OPEN, ElementStyle::ModeSearch),
        },
        FileBrowserMode::CreateFile => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_CREATE, ElementStyle::ModeEdit),
        },
        FileBrowserMode::ImportFile => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_IMPORT, ElementStyle::ModeSearch),
        },
        FileBrowserMode::ExportFile { .. } => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_EXPORT, ElementStyle::ModeEdit),
        },
        FileBrowserMode::ShareDestination => match state.filter {
            FileFilter::PwdKeeperOnly => (texts::MODE_PWD, ElementStyle::ModeNormal),
            FileFilter::All => (texts::MODE_SHARE, ElementStyle::ModeEdit),
        },
    };

    let hint = if state.search_active {
        format!("Search: {}█", state.search_query)
    } else if state.pending == AddPrefix::Pending {
        texts::FB_PROMPT_NEW.to_string()
    } else if state.focus == FocusTarget::Input {
        fb_input_hint(state)
    } else {
        fb_hint(state)
    };

    (mode_str, mode_style, hint)
}
/// The confirm/dismiss hint for the rename, name and share-folder inputs.
fn fb_input_hint(state: &FileBrowserState) -> String {
    let confirm = if state.mode.is_share_destination() {
        texts::FB_HINT_SHARE_CONFIRM
    } else {
        texts::FB_HINT_CONFIRM
    };
    format!("{}  {}", confirm, texts::FB_HINT_CANCEL)
}
/// Concatenates the key hints advertised by the file list for the current mode
/// and selection. Constants carry their own padding, so a plain `trim` suffices.
fn fb_hint(state: &FileBrowserState) -> String {
    let mut parts = Vec::new();

    // Import only accepts the exported formats; advertise them before the keys.
    if state.mode == FileBrowserMode::ImportFile {
        parts.push(texts::FB_HINT_IMPORT_TYPES);
    }

    // Export and share name a destination in the path bar; `n` edits it, and `e`
    // writes the export or commits the share into the directory now open.
    if state.mode.export_format().is_some() {
        parts.push(texts::FB_HINT_EXPORT_NAME);
        parts.push(texts::FB_HINT_EXPORT);
    }
    if state.mode.is_share_destination() {
        parts.push(texts::FB_HINT_SHARE_NAME);
        parts.push(texts::FB_HINT_SHARE);
    }

    parts.extend([
        texts::FB_HINT_FILTER,
        texts::FB_HINT_SEARCH,
        texts::FB_HINT_DISKS,
        texts::FB_HINT_HIDDEN,
    ]);

    if state.mode == FileBrowserMode::CreateFile {
        parts.push(texts::FB_HINT_NEW);
    }

    // Rename/delete hints only when a PWD-marked (age) file is selected.
    if let Some(entry) = state.entries.get(state.selected)
        && entry.is_age_file
        && !entry.is_dir
        && state.mode.allows_entry_actions()
    {
        parts.push(texts::FB_HINT_RENAME);
        parts.push(texts::FB_HINT_DELETE);
    }

    parts.concat().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{new_state, populate_tmp, share_state};
    use super::status_parts;
    use crate::file_browser::FileBrowserMode;
    use crate::file_browser::state::{AddPrefix, FileFilter, FocusTarget};
    use crate::file_handling::ExportFormat;
    use crate::theming::texts;

    #[test]
    fn status_parts_export_advertises_name_and_export_keys() {
        let mut state = new_state(FileBrowserMode::ExportFile {
            format: ExportFormat::Json,
        });
        populate_tmp(&mut state);

        let (mode, _, hint) = status_parts(&state);

        assert_eq!(mode, texts::MODE_EXPORT);
        assert!(
            hint.contains("[n]name"),
            "hint should advertise the name key: {hint}"
        );
        assert!(
            hint.contains("[e]export"),
            "hint should advertise the export key: {hint}"
        );
    }
    #[test]
    fn status_parts_open_file_shows_open_badge_and_filter_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let (mode, _, hint) = status_parts(&state);
        assert_eq!(
            mode,
            texts::MODE_OPEN,
            "the OPEN badge should lead the hint"
        );
        assert!(
            hint.contains("[f]"),
            "hint should advertise the filter toggle: {hint}"
        );
    }

    #[test]
    fn status_parts_pwd_filter_shows_pwd_badge() {
        for mode in [
            FileBrowserMode::OpenFile,
            FileBrowserMode::CreateFile,
            FileBrowserMode::ImportFile,
        ] {
            let mut state = new_state(mode);
            populate_tmp(&mut state);
            state.filter = FileFilter::PwdKeeperOnly;
            let (mode_label, _, _) = status_parts(&state);
            assert_eq!(
                mode_label,
                texts::MODE_PWD,
                "the PWD badge should replace the mode badge under the PwdKeeper filter in every mode ({mode:?})"
            );
        }
    }

    #[test]
    fn status_parts_mode_badge_shown_without_filter() {
        let mut create = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut create);
        assert_eq!(status_parts(&create).0, texts::MODE_CREATE);

        let mut import = new_state(FileBrowserMode::ImportFile);
        populate_tmp(&mut import);
        assert_eq!(status_parts(&import).0, texts::MODE_IMPORT);
    }

    #[test]
    fn status_parts_import_file_leads_with_supported_types() {
        let mut state = new_state(FileBrowserMode::ImportFile);
        populate_tmp(&mut state);
        let (mode, _, hint) = status_parts(&state);
        assert_eq!(mode, texts::MODE_IMPORT);
        assert!(
            hint.starts_with("[json/csv/xml]"),
            "the supported-format hint should lead the ImportFile hint: {hint}"
        );
    }

    #[test]
    fn status_parts_open_file_advertises_search_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("[/]search"),
            "hint should advertise search: {hint}"
        );
    }
    #[test]
    fn status_parts_name_input_shows_confirm_and_cancel() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let (mode, _, hint) = status_parts(&state);
        assert_eq!(mode, texts::MODE_CREATE);
        assert!(
            hint.contains("Enter:confirm") && hint.contains("Esc:cancel"),
            "the name input should route confirm/cancel to the status bar: {hint}"
        );
    }
    #[test]
    fn status_parts_search_active_shows_query() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.search_active = true;
        state.search_query = "test".to_string();
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("Search: test"),
            "the search prompt should be the hint: {hint}"
        );
    }
    #[test]
    fn status_parts_create_file_advertises_new_hint() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("[a]add"),
            "CreateFile should advertise the add file/folder hint: {hint}"
        );
    }

    #[test]
    fn status_parts_open_file_hides_new_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let (_, _, hint) = status_parts(&state);
        assert!(
            !hint.contains("[a]add"),
            "OpenFile must not advertise the add file/folder hint: {hint}"
        );
    }

    #[test]
    fn status_parts_pending_prefix_shows_new_file_prompt() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.pending = AddPrefix::Pending;
        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("f:add file") && hint.contains("d:add dir"),
            "the pending a-prefix prompt should be the hint: {hint}"
        );
    }
    #[test]
    fn status_parts_open_file_advertises_disks_and_hidden() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);

        let (_, _, hint) = status_parts(&state);
        assert!(
            hint.contains("[D]disks"),
            "hint should advertise the locations overlay: {hint}"
        );
        assert!(
            hint.contains("[Ctrl+h]hidden"),
            "hint should advertise the hidden toggle: {hint}"
        );
    }
    #[test]
    fn status_parts_share_advertises_the_name_and_commit_keys() {
        let dir = tempfile::tempdir().unwrap();
        let state = share_state(dir.path());

        let (mode, _, hint) = status_parts(&state);

        assert_eq!(mode, texts::MODE_SHARE);
        assert!(
            hint.contains("[n]name"),
            "the share browser should advertise the name key: {hint}"
        );
        assert!(
            hint.contains("[e]share"),
            "the share browser should advertise the commit key: {hint}"
        );
    }

    #[test]
    fn status_parts_share_input_names_the_enter_key() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = share_state(dir.path());
        state.focus = FocusTarget::Input;

        let (_, _, hint) = status_parts(&state);

        assert!(
            hint.contains("Enter:share"),
            "the share input should commit with Enter: {hint}"
        );
    }
}
