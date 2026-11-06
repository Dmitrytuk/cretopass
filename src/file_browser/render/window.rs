//! The File Browser's frame: the overlay rect, the title, the path bar and the
//! stacking order of its popups.

use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::file_browser::state::{FileBrowserMode, FileBrowserState, FocusTarget};
use crate::file_handling::ExportFormat;
use crate::theming::{ElementStyle, texts};

use super::list::render_file_list;
use super::popups::{
    render_delete_popup, render_error_popup, render_input_popup, render_locations_popup,
    render_overwrite_popup,
};

const DEFAULT_HEIGHT: u16 = 18;
/// Renders the file browser modal over the given screen area.
///
/// The renderer also maintains the list viewport offsets so the cursor moves
/// inside the scroll margin before the viewport follows it.
pub fn render_file_browser(frame: &mut Frame, area: Rect, state: &mut FileBrowserState) {
    state.clicks.clear();
    let overlay_area = make_overlay(area);

    // Clear overlay area first, then draw block borders on top
    frame.render_widget(Clear, overlay_area);

    let title: Cow<'static, str> = match state.mode {
        FileBrowserMode::OpenFile => Cow::Borrowed(texts::BROWSER_OPEN),
        FileBrowserMode::CreateFile => Cow::Borrowed(texts::BROWSER_CREATE),
        FileBrowserMode::ImportFile => Cow::Borrowed(texts::BROWSER_IMPORT),
        // The title names the format: the chosen extension replaces the typed
        // one, and the user should see that before confirming.
        FileBrowserMode::ExportFile { format } => Cow::Owned(format!(
            "{}{} ",
            texts::BROWSER_EXPORT,
            format_label(format)
        )),
        FileBrowserMode::ShareDestination => Cow::Borrowed(texts::BROWSER_SHARE),
    };

    let block = Block::default()
        .title(title.as_ref())
        .borders(Borders::ALL)
        .style(ElementStyle::DetailBlock.style());

    frame.render_widget(&block, overlay_area);

    let inner = block.inner(overlay_area);

    // Split the inner area into the path bar and the file list. The status bar
    // is not drawn here: the parent window (Home) owns it and renders the File
    // Browser's mode badge and hints through `status_parts`.
    let layout = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).split(inner);

    render_path_bar(frame, layout[0], state);
    render_file_list(frame, layout[1], state);

    // Transient prompts are centered popups stacked over the browser:
    // the active input / overwrite confirmation / delete confirmation first,
    // the error on top. The panel follows the topmost overlay, so an outside
    // click unwinds exactly one level, like `Esc`.
    let topmost = if state.focus == FocusTarget::Input && state.mode == FileBrowserMode::CreateFile
    {
        // `af` creates a file, `ad` creates a directory — match the title to it.
        let input_title = if state.creating_is_dir {
            texts::OVERLAY_NEW_DIR
        } else {
            texts::OVERLAY_NEW_FILE
        };
        render_input_popup(
            frame,
            overlay_area,
            input_title,
            texts::LABEL_NAME_INLINE,
            &state.creating_name,
            state.creating_cursor,
        )
    } else if state.focus == FocusTarget::Input && state.mode.export_format().is_some() {
        render_input_popup(
            frame,
            overlay_area,
            texts::OVERLAY_EXPORT_FILE,
            texts::LABEL_NAME_INLINE,
            &state.creating_name,
            state.creating_cursor,
        )
    } else if state.focus == FocusTarget::Input && state.mode.is_share_destination() {
        render_input_popup(
            frame,
            overlay_area,
            texts::OVERLAY_SHARE_FOLDER,
            texts::LABEL_FOLDER_INLINE,
            &state.creating_name,
            state.creating_cursor,
        )
    } else if state.focus == FocusTarget::Rename {
        render_input_popup(
            frame,
            overlay_area,
            texts::OVERLAY_RENAME,
            texts::LABEL_RENAME,
            &state.rename_name,
            state.rename_cursor,
        )
    } else if state.confirm_overwrite.is_some() {
        render_overwrite_popup(frame, overlay_area, state).unwrap_or(overlay_area)
    } else if state.confirm_delete.is_some() {
        render_delete_popup(frame, overlay_area, state).unwrap_or(overlay_area)
    } else if state.focus == FocusTarget::Locations {
        // Centred on the full frame so the disk list has room. `centered_popup_sized`
        // keeps the popup off the bottom row, which is where the parent window's
        // status bar lives, so the hint stays visible.
        render_locations_popup(frame, area, state)
    } else {
        overlay_area
    };
    state.clicks.set_panel(topmost);

    if let Some(error_area) = render_error_popup(frame, overlay_area, state) {
        state.clicks.set_panel(error_area);
    }
}
fn make_overlay(area: Rect) -> Rect {
    let height = DEFAULT_HEIGHT.min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 8,
        y: (area.height.saturating_sub(height)) / 2,
        width: area.width * 6 / 8,
        height,
    }
}
fn render_path_bar(frame: &mut Frame, area: Rect, state: &FileBrowserState) {
    // Export and share name their destination in the bar: the export resolves
    // the format's extension, while a share shows the folder it writes into.
    let target = state.export_target().or_else(|| state.share_target());
    let (dir, file_name) = match target.as_deref() {
        Some(target) => {
            let dir = target
                .parent()
                .filter(|dir| !dir.as_os_str().is_empty())
                .unwrap_or(state.current_dir.as_path());
            (dir, target.file_name())
        }
        None => (state.current_dir.as_path(), None),
    };

    let mut spans = vec![Span::styled(
        format!(" {}", dir.to_string_lossy()),
        ElementStyle::FbPathBar.style(),
    )];

    if let Some(name) = file_name {
        spans.push(Span::styled(
            std::path::MAIN_SEPARATOR.to_string(),
            ElementStyle::FbPathBar.style(),
        ));
        spans.push(Span::styled(
            name.to_string_lossy().into_owned(),
            ElementStyle::FbExportName.style(),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
/// The human-readable name of `format`, for the browser's title.
fn format_label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Json => texts::FORMAT_JSON,
        ExportFormat::Xml => texts::FORMAT_XML,
        ExportFormat::Csv => texts::FORMAT_CSV,
    }
}

#[cfg(test)]
mod tests {
    use super::super::chrome::{centered_popup, centered_popup_sized};
    use super::super::fixtures::{new_state, populate_tmp, render_to_text, share_state};
    use super::make_overlay;
    use crate::file_browser::FileBrowserMode;
    use crate::file_browser::Location;
    use crate::file_browser::state::FocusTarget;
    use crate::file_handling;
    use crate::file_handling::ExportFormat;
    use ratatui::layout::Rect;
    use std::path::PathBuf;

    #[test]
    fn the_topmost_popup_becomes_the_click_panel() {
        let area = Rect::new(0, 0, 80, 24);
        let overlay = make_overlay(area);
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);

        let _ = render_to_text(&mut state);
        assert_eq!(
            state.clicks.panel(),
            Some(overlay),
            "with no popup, the browser overlay is the panel"
        );

        state.focus = FocusTarget::Input;
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.clicks.panel(),
            Some(centered_popup(overlay, 4)),
            "the focused name input becomes the topmost panel"
        );

        state.locations = (0..20)
            .map(|i| Location {
                label: format!("loc{i}"),
                path: PathBuf::from(format!("/mnt/loc{i}")),
                detail: String::new(),
                is_home: false,
            })
            .collect();
        state.focus = FocusTarget::Locations;
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.clicks.panel(),
            Some(centered_popup_sized(area, 22, 3, 4, 40)),
            "the Locations picker outranks the browser overlay"
        );

        state.locations.clear();
        state.focus = FocusTarget::List;
        state.confirm_delete = Some("/tmp/gone.age".to_string());
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.clicks.panel(),
            Some(centered_popup(overlay, 4)),
            "the delete confirmation becomes the panel"
        );

        state.confirm_delete = None;
        state.confirm_overwrite = Some("/tmp/existing.json".to_string());
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.clicks.panel(),
            Some(centered_popup(overlay, 4)),
            "the overwrite confirmation becomes the panel"
        );

        state.confirm_overwrite = None;
        state.error = Some("boom".to_string());
        let _ = render_to_text(&mut state);
        assert_eq!(
            state.clicks.panel(),
            Some(centered_popup(overlay, 4)),
            "the error popup is the topmost panel"
        );
    }

    #[test]
    fn test_render_open_file_shows_path_bar() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&mut state);
        assert!(
            text.contains("/tmp"),
            "path bar should show '/tmp', got:\n{text}"
        );
    }

    #[test]
    fn test_render_export_shows_the_resolved_file_name_in_the_path_bar() {
        let mut state = new_state(FileBrowserMode::ExportFile {
            format: ExportFormat::Csv,
        });
        populate_tmp(&mut state);
        state.creating_name = "backup.txt".to_string();

        let text = render_to_text(&mut state);

        assert!(
            text.contains("backup.csv"),
            "the path bar should name the file the format writes:\n{text}"
        );
    }

    #[test]
    fn test_render_export_hides_the_name_until_one_is_typed() {
        let mut state = new_state(FileBrowserMode::ExportFile {
            format: ExportFormat::Json,
        });
        populate_tmp(&mut state);
        state.creating_name.clear();

        let text = render_to_text(&mut state);

        assert!(
            !text.contains(".json"),
            "a blank name must not append an extension to the path bar:\n{text}"
        );
    }
    #[test]
    fn test_render_open_file_shows_entries() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&mut state);
        // At least one entry name should appear in the output
        let has_entry = state.entries.iter().any(|e| text.contains(&e.name));
        assert!(has_entry, "no entry name found in rendered output:\n{text}");
    }
    #[test]
    fn test_render_create_file_no_name_input_when_focus_is_list() {
        // In CreateFile mode but focus is on list → no name input area
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::List;
        let text = render_to_text(&mut state);
        // Title should be "Create New Secret File"
        assert!(
            text.contains("Create New Secret File"),
            "title mismatch:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_shows_name_input_when_focused() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let text = render_to_text(&mut state);
        assert!(
            text.contains("Name:"),
            "should show Name: input when focused:\n{text}"
        );
    }
    #[test]
    fn test_render_create_file_name_input_is_popup() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        let text = render_to_text(&mut state);
        assert!(
            text.contains("New File"),
            "name input should render as a titled popup:\n{text}"
        );
    }

    #[test]
    fn test_render_create_dir_name_input_titled_new_folder() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        state.creating_is_dir = true;
        let text = render_to_text(&mut state);
        assert!(
            text.contains("New Folder"),
            "the `ad` name input should be titled as a folder:\n{text}"
        );
        assert!(
            !text.contains("New File"),
            "the `ad` name input must not be titled as a file:\n{text}"
        );
    }

    #[test]
    fn test_render_rename_input_is_popup() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Rename;
        state.rename_name = "renamed.age".to_string();
        state.rename_cursor = state.rename_name.len();
        let text = render_to_text(&mut state);
        assert!(
            text.contains("Rename"),
            "rename input should render as a titled popup:\n{text}"
        );
        assert!(
            text.contains("renamed.age"),
            "popup should show the name being edited:\n{text}"
        );
    }

    #[test]
    fn test_render_delete_confirm_is_popup() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.confirm_delete = Some("/tmp/secret.age".to_string());
        let text = render_to_text(&mut state);
        assert!(
            text.contains("Confirm Delete"),
            "delete confirmation should render as a titled popup:\n{text}"
        );
        assert!(
            text.contains("Delete secret.age?"),
            "popup should name the file to delete:\n{text}"
        );
    }

    #[test]
    fn test_render_error_is_popup() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.error = Some("Permission denied".to_string());
        let text = render_to_text(&mut state);
        assert!(
            text.contains("Alert"),
            "error should render as a titled popup:\n{text}"
        );
    }

    #[test]
    fn test_render_create_file_with_partial_name() {
        let mut state = new_state(FileBrowserMode::CreateFile);
        populate_tmp(&mut state);
        state.focus = FocusTarget::Input;
        state.creating_name = "myvault.age".to_string();
        state.creating_cursor = 5;
        let text = render_to_text(&mut state);
        assert!(
            text.contains("myvault"),
            "should show the typed name:\n{text}"
        );
    }
    #[test]
    fn test_render_error_bar_shows_message() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.error = Some("Permission denied".to_string());
        let text = render_to_text(&mut state);
        assert!(
            text.contains("Permission denied"),
            "should show error text:\n{text}"
        );
    }

    #[test]
    fn test_render_empty_list_shows_empty_directory_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        // A known empty directory renders the empty-state message.
        let empty_dir = std::path::Path::new("/tmp/__test_empty_xyz__");
        let _ = std::fs::remove_dir_all(empty_dir);
        let _ = std::fs::create_dir_all(empty_dir);
        state.current_dir = empty_dir.to_path_buf();
        state.entries = file_handling::list_dir(empty_dir).unwrap();

        let text = render_to_text(&mut state);

        assert!(
            text.contains("(empty directory)"),
            "an empty directory should show the empty state message:\n{text}"
        );
        let _ = std::fs::remove_dir(empty_dir);
    }

    #[test]
    fn test_render_non_matching_search_shows_no_matching_files_hint() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        state.search_query = "no-such-name-xyz".to_string();

        let text = render_to_text(&mut state);

        assert!(
            text.contains("No matching files"),
            "a search that matches nothing should show the no-match message:\n{text}"
        );
    }
    #[test]
    fn test_render_dirs_marked_with_arrow_icon() {
        let mut state = new_state(FileBrowserMode::OpenFile);
        populate_tmp(&mut state);
        let text = render_to_text(&mut state);
        // The selected entry should show ▸ prefix
        let has_arrow = text.contains('▸');
        assert!(has_arrow, "should have selection arrow (▸):\n{text}");
    }

    #[test]
    fn test_render_pwd_marker_shown_for_age_files() {
        let dir = std::path::Path::new("/tmp/__test_render_pwd__");
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();

        // Create an age file
        let age_path = dir.join("myvault.age");
        let data = [b"age-encrypt".as_slice(), &[0u8, 1, 2, 3]].concat();
        std::fs::write(&age_path, &data).unwrap();

        // Create a regular file too
        let txt_path = dir.join("notes.txt");
        std::fs::write(&txt_path, b"hello").unwrap();

        let mut state = new_state(FileBrowserMode::OpenFile);
        state.current_dir = dir.to_path_buf();
        state.entries = file_handling::list_dir(dir).unwrap();
        state.selected = state
            .entries
            .iter()
            .position(|e| e.name == "myvault.age")
            .unwrap_or(0);

        let text = render_to_text(&mut state);

        assert!(
            text.contains("[PWD]"),
            "should show [PWD] marker for age files:\n{text}"
        );
        assert!(
            text.contains("myvault.age"),
            "should show age filename:\n{text}"
        );
        assert!(
            text.contains("notes.txt"),
            "should show regular filename:\n{text}"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
    #[test]
    fn test_render_share_titles_the_browser() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = share_state(dir.path());

        let text = render_to_text(&mut state);

        assert!(
            text.contains("Share into"),
            "the share browser should name its purpose:\n{text}"
        );
    }

    #[test]
    fn test_render_share_shows_the_folder_in_the_path_bar() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = share_state(dir.path());

        let text = render_to_text(&mut state);

        assert!(
            text.contains("vault"),
            "the path bar should name the folder about to be written:\n{text}"
        );
    }
}
