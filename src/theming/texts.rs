pub const TITLE_TREE_DEFAULT: &str = " File ";
pub const TITLE_TREE_PREFIX: &str = " File — ";
pub const TITLE_HOME_DEFAULT: &str = " Home ";
pub const TITLE_HOME_PREFIX: &str = " Home — ";

// ── App identity & feedback ──
//
// One source of truth for the app's name, version, tagline, and feedback
// links, shared by the TUI About overlay and the CLI `--help` footer so the
// two can never drift apart (see
// [ADR 0020](../../../docs/adr/0020-feedback-and-issue-reporting.md)).

pub const APP_NAME: &str = "cretopass";
/// One-line description of the app shown in the About overlay and `--help`.
pub const APP_TAGLINE: &str = "Terminal-based secret keeper with age-encrypted storage";
/// Version from `Cargo.toml`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// The single channel for ordinary bug reports and feature requests.
pub const ISSUES_URL: &str = "https://github.com/Dmitrytuk/cretopass/issues";
/// The project's source repository and homepage.
pub const PROJECT_URL: &str = "https://github.com/Dmitrytuk/cretopass";
/// The one-line installer for Linux and macOS.
pub const INSTALL_SH_URL: &str =
    "https://github.com/Dmitrytuk/cretopass/releases/latest/download/install.sh";
/// The one-line installer for Windows.
pub const INSTALL_PS1_URL: &str =
    "https://github.com/Dmitrytuk/cretopass/releases/latest/download/install.ps1";
/// GitHub's private vulnerability reporting form for security issues.
pub const SECURITY_ADVISORY_URL: &str =
    "https://github.com/Dmitrytuk/cretopass/security/advisories/new";
/// The license name.
pub const LICENSE_NAME: &str = "MIT";
/// The license text in the repository.
pub const LICENSE_URL: &str = "https://github.com/Dmitrytuk/cretopass/blob/master/LICENSE";

pub const OVERLAY_SEARCH: &str = " Search ";
pub const OVERLAY_ADD_NODE: &str = "Add Node";
pub const OVERLAY_EDIT_NODE: &str = "Edit Node";
pub const OVERLAY_SELECT_FIELD_TYPE: &str = " Select Field Type ";
pub const OVERLAY_CUSTOM_FIELD_NAME: &str = " Custom Field Name ";
pub const OVERLAY_GENERATE_PASSWORD: &str = " Generate Password ";
pub const OVERLAY_SYMBOLS: &str = " Special Characters ";
pub const OVERLAY_CONFIRM_DELETE: &str = " Confirm Delete ";
pub const OVERLAY_HELP: &str = " Help ";
pub const OVERLAY_ABOUT: &str = " About ";
pub const OVERLAY_SAVE_ERROR: &str = " Save Error ";

// Labels used by the About overlay and the CLI `--help` footer.
pub const ABOUT_LABEL_BUGS: &str = "Bug reports & feature requests:";
pub const ABOUT_LABEL_SECURITY: &str = "Security vulnerabilities (private):";
pub const ABOUT_LICENSE_PREFIX: &str = "License: ";

pub const CONFIG_TITLE: &str = " Configuration ";
pub const HOME_CHANGE_PASSWORD: &str = " Change Password ";
pub const HOME_CREATE_SECRET: &str = " Create Secret File ";
pub const HOME_NO_PASSPHRASE: &str = " No Passphrase ";
pub const HOME_ENTER_PASSPHRASE: &str = " Enter Passphrase ";
pub const ALERT_TITLE: &str = " Alert ";
pub const HOME_DELETE_CONFIRM: &str = " Confirm Delete ";

pub const BROWSER_OPEN: &str = " Open File ";
pub const BROWSER_CREATE: &str = " Create New Secret File ";
pub const BROWSER_IMPORT: &str = " Import File ";
pub const BROWSER_EXPORT: &str = " Export as ";
pub const OVERLAY_NEW_FILE: &str = " New File ";
pub const OVERLAY_NEW_DIR: &str = " New Folder ";
pub const OVERLAY_EXPORT_FILE: &str = " Export File ";
pub const OVERLAY_CONFIRM_OVERWRITE: &str = " Confirm Overwrite ";
pub const OVERLAY_RENAME: &str = " Rename ";
pub const OVERLAY_LOCATIONS: &str = " Locations ";
pub const LOCATION_HOME: &str = "Home";
pub const LOCATION_USB: &str = "[USB]";
pub const LOCATION_READ_ONLY: &str = "[RO]";

pub const MENU_ITEMS: &[&str] = &[
    "Open secret file",
    "Create new secret file",
    "Configuration",
    "Change file password",
    "Export active file",
    "Share active file",
    "Import from file",
];

pub const SETTING_FULL_PATH: &str = "Display full path in title bar";
pub const SETTING_LAST_FILE: &str = "Start with last file";
pub const SETTING_PALETTE: &str = "Colour scheme";
pub const PALETTE_DARK: &str = "Dark";
pub const PALETTE_LIGHT: &str = "Light";

pub const FIELD_ITEM_CUSTOM: &str = "Custom Field";
pub const FIELD_ITEM_DESC: &str = "Description";

pub const EXPORT_SELECT_TITLE: &str = " Select Export Format ";
pub const EXPORT_TYPE_ITEMS: [&str; 3] = ["1. JSON", "2. XML", "3. CSV"];
pub const FORMAT_JSON: &str = "JSON";
pub const FORMAT_XML: &str = "XML";
pub const FORMAT_CSV: &str = "CSV";

pub const LABEL_NAME: &str = "Name: ";
pub const LABEL_PASSPHRASE: &str = "Passphrase: ";
pub const LABEL_PASSPHRASE_FIELD: &str = "Passphrase";
pub const LABEL_FILE: &str = "File: ";
pub const LABEL_CURRENT_PW: &str = "Current password";
pub const LABEL_NEW_PW: &str = "New password";
pub const LABEL_CONFIRM_PW: &str = "Confirm new password";
pub const LABEL_PASSPHRASE_CONFIRM: &str = "Confirm passphrase";
pub const LABEL_RENAME: &str = " Rename: ";
pub const LABEL_NAME_INLINE: &str = " Name: ";
pub const LABEL_GENERATOR_LENGTH: &str = "Length: ";
pub const LABEL_GENERATOR_SYMBOLS: &str = "Special characters";
pub const LABEL_GENERATOR_RUN: &str = "Generate";

pub const MODE_NORMAL: &str = "NORMAL";
pub const MODE_NAV: &str = "NAV";
pub const MODE_SEARCH: &str = "SEARCH";
pub const MODE_PENDING_CUT: &str = "CUT";
pub const MODE_PASTE: &str = "PASTE";
pub const MODE_PENDING_ADD: &str = "ADD";
pub const MODE_DETAIL: &str = "DETAIL";
pub const MODE_EDIT: &str = "EDIT";
pub const MODE_ADD: &str = "ADD";
pub const MODE_FIELD_TYPE: &str = "FIELD TYPE";
pub const MODE_CUSTOM_NAME: &str = "CUSTOM NAME";
pub const MODE_GENERATOR: &str = "GENERATE";
pub const MODE_SYMBOLS: &str = "SYMBOLS";
pub const MODE_CONFIRM: &str = "CONFIRM";
pub const MODE_HELP: &str = "HELP";
pub const MODE_ABOUT: &str = "ABOUT";
pub const MODE_MENU: &str = "MENU";
pub const MODE_OPEN: &str = "OPEN";
pub const MODE_CREATE: &str = "CREATE";
pub const MODE_IMPORT: &str = "IMPORT";
pub const MODE_PASSPHRASE: &str = "PASSPHRASE";
pub const MODE_CONFIG: &str = "CONFIG";
pub const MODE_DELETE: &str = "DELETE";
pub const MODE_CHANGE_PW: &str = "CHANGE PW";
pub const MODE_EXPORT: &str = "EXPORT";

pub const STATUS_LOADING: &str = "Loading";
pub const STATUS_SAVING: &str = "Saving";
pub const STATUS_SAVE_FAILED: &str = "Save failed";

pub const HINT_TREE: &str =
    "j/↓ k/↑:nav  h/← l/→:fold  d:cut  a:add  /:search  H:home  ?:help  q:quit";
/// Tree hint shown while the cut buffer holds a node ready to paste.
pub const HINT_TREE_PASTE: &str =
    "j/↓ k/↑:nav  h/← l/→:fold  d:cut  p:paste  a:add  /:search  H:home  ?:help  q:quit";
/// Tree hint shown when the Root is selected: it has no sibling slot, so
/// `d:cut` is not offered.
pub const HINT_TREE_ROOT: &str =
    "j/↓ k/↑:nav  h/← l/→:fold  a:add  /:search  H:home  ?:help  q:quit";
/// Root variant of [`HINT_TREE_PASTE`]: pasting a child into the Root still
/// applies, pasting a sibling does not.
pub const HINT_TREE_ROOT_PASTE: &str =
    "j/↓ k/↑:nav  h/← l/→:fold  p:paste  a:add  /:search  H:home  ?:help  q:quit";
pub const HINT_SEARCH: &str = "Type to search  ^V:paste  Enter:done  Esc:cancel";
pub const HINT_PENDING_CUT: &str = "d:cut  Esc:cancel";
pub const HINT_PASTE: &str = "p:after  P:before  l/→:child  Esc:cancel";
/// Paste hint when the Root is selected: only a child can be pasted into it.
pub const HINT_PASTE_ROOT: &str = "l/→:child  Esc:cancel";
pub const HINT_PENDING_ADD: &str = "a:after  A:before  l/→:child  Esc:cancel";
/// Add hint when the Root is selected: only a child can be added to it.
pub const HINT_PENDING_ADD_ROOT: &str = "l/→:child  Esc:cancel";
pub const HINT_DETAIL: &str = "j/k/Tab:navigate  c/y:copy  i/Enter:edit  Esc:close";
pub const HINT_DETAIL_ALERT: &str = "Press any key to continue";
pub const HINT_FORM: &str = "j/k:field  i/Enter:edit  u/r:undo/redo  Esc:close";
pub const HINT_FORM_EXTRA: &str =
    "j/k:field  i/Enter:edit  dd:cut  Del:delete  u/r:undo/redo  Esc:close";
/// Form hint shown while the field cut buffer holds a field ready to paste.
pub const HINT_FORM_EXTRA_PASTE: &str =
    "j/k:field  i/Enter:edit  dd:cut  p:paste  Del:delete  u/r:undo/redo  Esc:close";
pub const HINT_FORM_ADD: &str = "Enter:add field  Esc:cancel";
pub const HINT_FORM_INPUT: &str = "Type to edit  ^V:paste  Enter/Esc:done";
pub const HINT_FORM_GENERATOR_INPUT: &str = "Type to edit  ^V:paste  ^G:generate  Enter/Esc:done";
pub const HINT_GENERATOR: &str =
    "j/k:row  digits:length  ←/→:cursor  Space:all  Enter:ok  Esc:cancel";
pub const HINT_SYMBOLS: &str = "h/l/j/k:move  Space/Enter:toggle  Esc:done";
pub const HINT_FIELD_CUT: &str = "d:cut field  Esc:cancel";
pub const HINT_FIELD_PASTE: &str = "p:after  P:before  Esc:cancel";
pub const HINT_SELECT_FIELD_TYPE: &str = "j/k:nav  1-9:select  Enter:choose  Esc:cancel";
pub const HINT_CUSTOM_FIELD_NAME: &str = "Type name  ^V:paste  Enter:done  Esc:cancel";
pub const HINT_CONFIRM_DELETE: &str = "y:delete  n/Esc:cancel";
pub const HINT_HOME: &str = "j/k:navigate  Enter/l:select  d:<delete path>  ?:about  q:quit";
pub const HINT_CONFIG: &str = "Space:toggle  Enter:save  Esc:cancel";
pub const HINT_DELETE_CONFIRM: &str = "y/Enter:confirm  any key:cancel";
pub const HINT_CHANGE_PW_CONFIRM: &str = "y/Enter:yes  any key:no";
pub const HINT_CHANGE_PW_INPUT: &str =
    "Tab/↑/↓:switch  ^V:paste  Ctrl+S:show  Enter:apply  Esc:cancel";
pub const HINT_NO_PASSPHRASE: &str = "y/Enter:yes  any key:no";
pub const HINT_PASSPHRASE: &str = "Enter:confirm  ^V:paste  Ctrl+S:show  Esc:cancel";
pub const HINT_CREATE_SECRET: &str =
    "Tab/↑/↓:switch  ^V:paste  Ctrl+S:show  Enter:create  Esc:cancel";
pub const HINT_ALERT: &str = "Press any key to dismiss";
pub const HINT_HELP: &str = "Press any key to close";
pub const HINT_SAVE_EXIT_BLOCKED: &str = "r:retry  q:quit without saving  any other key:stay";
pub const HINT_DISCARD_CONFIRM: &str = "y:quit without saving  any other key:stay";

pub const TREE_HELP_KEY_BINDINGS: &[&str] = &[
    " j/k      Navigate up/down",
    " ↓/↑      Navigate up/down",
    " h/←      Collapse, or jump to parent",
    " l/→      Expand (parent) or detail (leaf)",
    " Enter    Toggle expand (parent) or detail",
    " i/Tab    Detail overlay for any node",
    " d d      Cut selected node",
    " p        Paste: p after, P before, l/→ child",
    " a        Add: a after, A before, l/→ child",
    " Root     No cut, no sibling add/paste",
    " Ctrl+e e Export selected node",
    " Ctrl+e s Share selected node",
    " ^V       Paste from the system clipboard",
    " Mouse    Wheel:scroll  Left:select/act",
    " /        Search",
    " ?        Show this help",
    " Esc      Clear search highlights",
    " q        Quit the app",
    " H        Return to home screen",
];

pub const PROMPT_DELETE_FIELD_START: &str = "Delete \"";
pub const PROMPT_DELETE_FIELD_END: &str = "\" field? (y/N)";
pub const PROMPT_DELETE_RECENT_START: &str = "Remove '";
pub const PROMPT_DELETE_RECENT_END: &str = "' from recent files?";
pub const PROMPT_CHANGE_PW: &str = "Are you sure you want to change the password?";
pub const PROMPT_NO_PASSPHRASE: &str = "You are creating the secret file without password?";

pub const OVERLAY_EXPORT_NODE: &str = " Export Node ";
pub const EXPORT_NODE_OPTIONS: [&str; 2] =
    ["1. Export this node", "2. Export this node with children"];
pub const HINT_EXPORT_NODE_SELECT: &str = "j/k:nav  Enter:choose  Esc:cancel";
pub const HINT_EXPORT_NODE_FORMAT: &str = "j/k:nav  Enter:choose  Esc:cancel";
pub const HINT_EXPORT_SELECT: &str = "j/k:nav  1-9:select  Enter:choose  Esc:cancel";
pub const MSG_EXPORT_NODE_SUCCESS: &str = "Node exported at ";
pub const MODE_EXPORT_SELECT: &str = "EXPORT";
pub const MODE_EXPORT_FORMAT: &str = "EXPORT FMT";

// ── Share ──
//
// Sharing writes a self-contained bundle: a new encrypted secret file for the
// chosen scope plus a HOW-TO-OPEN instruction. The File Browser names the
// bundle folder (see [ADR 0023](../../../docs/adr/0023-sharing-a-vault-subtree.md)).

pub const MODE_PENDING_EXPORT: &str = "EXPORT";
pub const HINT_PENDING_EXPORT: &str = "e:export  s:share  Esc:cancel";

pub const OVERLAY_SHARE_SCOPE: &str = " Share Node ";
pub const SHARE_SCOPE_OPTIONS: [&str; 2] =
    ["1. Share this node", "2. Share this node with children"];
pub const HINT_SHARE_SCOPE: &str = "j/k:nav  1-2:select  Enter:choose  Esc:cancel";
pub const MODE_SHARE: &str = "SHARE";

pub const BROWSER_SHARE: &str = " Share into ";
pub const OVERLAY_SHARE_FOLDER: &str = " Share Folder ";
pub const LABEL_FOLDER_INLINE: &str = " Folder: ";
pub const FB_HINT_SHARE_NAME: &str = " [n]name ";
pub const FB_HINT_SHARE: &str = " [e]share ";
pub const FB_HINT_SHARE_CONFIRM: &str = "Enter:share";
pub const FB_PROMPT_SHARE_FOLDER: &str = " Write into ";

pub const SHARE_TITLE_PASSPHRASE: &str = " Share Bundle Password ";
pub const HINT_SHARE_PASSPHRASE: &str =
    "Tab/↑/↓:switch  ^V:paste  Ctrl+S:show  Enter:share  Esc:cancel";
pub const PROMPT_SHARE_NO_PASSPHRASE: &str = "Create the share bundle without a password?";
pub const HINT_SHARE_NO_PASSPHRASE: &str = "y/Enter:yes  any key:no";
pub const MODE_SHARE_CONFIRM: &str = "CONFIRM";
pub const OVERLAY_SHARE_CONFIRM: &str = " Confirm Share ";

pub const MSG_SHARE_SUCCESS: &str = "Share bundle written at ";
pub const MSG_CANNOT_SHARE_INTO_FILE: &str = "Cannot share into an existing file";

// The instruction file written into every share bundle. The password is never
// part of it: it travels over a separate channel.
pub const SHARE_README_TITLE: &str = "cretopass — share bundle";
pub const SHARE_README_PROJECT_LABEL: &str = "Project: ";
pub const SHARE_README_INSTALL_UNIX_LABEL: &str = "Install (Linux/macOS): ";
pub const SHARE_README_INSTALL_WINDOWS_LABEL: &str = "Install (Windows): ";
pub const SHARE_README_FILE_LABEL: &str = "Secret file: ";
pub const SHARE_README_STEPS_HEADING: &str = "To open it:";
pub const SHARE_README_STEP_INSTALL: &str = "  1. Install and start cretopass.";
pub const SHARE_README_STEP_OPEN: &str = "  2. Choose \"Open secret file\" on the Home screen.";
pub const SHARE_README_STEP_PASSWORD: &str =
    "  3. Open the secret file and enter the password you received separately.";
pub const SHARE_README_STEP_NO_PASSWORD: &str = "  3. Open the secret file — it has no password.";
pub const SHARE_README_PASSWORD_NOTE: &str =
    "The password is never stored in this folder; ask the sender for it over a separate channel.";
pub const SHARE_README_FEEDBACK_LABEL: &str = "Feedback & bug reports: ";

/// The `HOW-TO-OPEN.txt` text written into a share bundle whose secret file is
/// `file_name`.
///
/// The password is never part of the text: a protected bundle points the
/// recipient at a separate channel, and a passwordless one says plainly that no
/// password is needed.
pub fn share_instruction(file_name: &str, passwordless: bool) -> String {
    let install_unix = format!("curl -fsSL {INSTALL_SH_URL} | sh");
    let install_windows = format!("irm {INSTALL_PS1_URL} | iex");
    let step = if passwordless {
        SHARE_README_STEP_NO_PASSWORD
    } else {
        SHARE_README_STEP_PASSWORD
    };

    let mut lines = vec![
        SHARE_README_TITLE.to_string(),
        String::new(),
        APP_TAGLINE.to_string(),
        String::new(),
        format!("{SHARE_README_PROJECT_LABEL}{PROJECT_URL}"),
        format!("{SHARE_README_INSTALL_UNIX_LABEL}{install_unix}"),
        format!("{SHARE_README_INSTALL_WINDOWS_LABEL}{install_windows}"),
        String::new(),
        format!("{SHARE_README_FILE_LABEL}{file_name}"),
        String::new(),
        SHARE_README_STEPS_HEADING.to_string(),
        SHARE_README_STEP_INSTALL.to_string(),
        SHARE_README_STEP_OPEN.to_string(),
        step.to_string(),
    ];

    if !passwordless {
        lines.push(String::new());
        lines.push(SHARE_README_PASSWORD_NOTE.to_string());
    }

    lines.push(String::new());
    lines.push(format!("{SHARE_README_FEEDBACK_LABEL}{ISSUES_URL}"));
    lines.push(String::new());

    lines.join("\n")
}

pub const MSG_NOT_IMPLEMENTED: &str = "This feature is not yet implemented";
pub const MSG_NO_OPEN_FILE: &str = "No file is currently open";
pub const MSG_CANNOT_DELETE_OPEN: &str = "Cannot delete the currently open file from recent files";
pub const MSG_CANNOT_WRITE: &str = "Cannot write to this path";
pub const MSG_FILE_EXISTS: &str = "A file with that name already exists";
pub const MSG_WRONG_MASTER_PW: &str = "Wrong master password, or the file was altered";
pub const MSG_WORK_FACTOR_TOO_HIGH: &str =
    "This secret file needs more computing power than this device allows";
pub const MSG_DAMAGED_SECRET: &str = "This secret file is damaged or unreadable";
pub const MSG_NOT_SECRET_FILE: &str = "Not a cretopass secret file";
pub const MSG_FILE_ERROR: &str = "File error: ";
pub const MSG_CURRENT_PW_INCORRECT: &str = "Current password is incorrect";
pub const MSG_PW_EMPTY: &str = "New password fields cannot be empty";
pub const MSG_PW_MISMATCH: &str = "New passwords do not match";
pub const MSG_CONFIG_SAVED: &str = "Settings saved";
pub const MSG_SAVE_FAILED: &str = "Failed to save: ";
pub const MSG_DISCARD_CONFIRM: &str = "Unsaved changes could not be saved. Quit anyway? [y/N]";
pub const MSG_EXPORT_SUCCESS: &str = "File exported at ";
pub const MSG_IMPORT_SUCCESS: &str = "File imported successfully - you can find it in the tree";
pub const MSG_IMPORT_NO_OPEN_FILE: &str = "Import requires an open file. Open one first.";
pub const IMPORT_NAME_FALLBACK: &str = "unknown";
/// Prefix of the wrapper node an imported file is filed under.
pub const IMPORT_NAME_PREFIX: &str = "imported_";
/// `chrono` format for the timestamp that keeps repeated imports apart.
pub const IMPORT_NAME_STAMP_FORMAT: &str = "%Y-%m-%d_%H%M%S";
pub const MSG_NO_FILE_PATH: &str = "No file path set";

pub const EMPTY_DIR: &str = "  (empty directory)";
pub const NO_MATCHING_FILES: &str = "  No matching files";
/// Shown in place of the file list while a directory listing is in flight.
pub const FB_LOADING: &str = "  Loading…";
pub const ADD_FIELD_BUTTON: &str = "+ Add Field";
pub const PASSWORD_MASK: &str = "*****";
pub const COPY_CHECK: &str = "✓ Copied!";
pub const MSG_COPY_FAILED: &str = "Failed to copy: ";
pub const SEPARATOR_RECENT_FILES: &str = "─── Recent Files ───";
pub const NEW_FOLDER_DEFAULT: &str = "New Folder";
pub const DEFAULT_NAME: &str = "Untitled";
pub const DEFAULT_ROOT_NAME: &str = "Secrets";
pub const CUSTOM_FIELD_DEFAULT: &str = "Custom";
pub const FIELD_LOGIN: &str = "Login";
pub const FIELD_PASSWORD: &str = "Password";
pub const FB_HINT_IMPORT_TYPES: &str = " [json/csv/xml] ";
pub const FB_HINT_EXPORT_NAME: &str = " [n]name ";
pub const FB_HINT_EXPORT: &str = " [e]export ";
pub const FB_HINT_SEARCH: &str = " [/]search ";
pub const FB_HINT_NEW: &str = " [a]add ";
pub const FB_HINT_RENAME: &str = " [r]rename ";
pub const FB_HINT_DELETE: &str = " [d]delete ";
pub const FB_PROMPT_NEW: &str = " f:add file  d:add dir  Esc:cancel ";
pub const FB_HINT_CONFIRM: &str = "Enter:confirm";
pub const FB_HINT_CANCEL: &str = "Esc:cancel";
pub const FB_HINT_DELETE_CONFIRM: &str = "y/Enter:confirm  any key:cancel";
pub const FB_HINT_OVERWRITE_CONFIRM: &str = "y/Enter:overwrite  any key:cancel";
/// Search-input prompt prefix; the live query and [`FB_SEARCH_CURSOR`] follow.
pub const FB_SEARCH_PREFIX: &str = "Search: ";
/// The block glyph marking the cursor in the search input.
pub const FB_SEARCH_CURSOR: &str = "█";
/// Delete-confirmation prompt; the file name sits between `_START` and `_END`
/// (mirrors [`PROMPT_DELETE_FIELD_START`]).
pub const FB_CONFIRM_DELETE_START: &str = " Delete ";
pub const FB_CONFIRM_DELETE_END: &str = "? ";
/// Overwrite-confirmation prompt; the file name sits between `_START` and `_END`.
pub const FB_CONFIRM_OVERWRITE_START: &str = " Overwrite ";
pub const FB_CONFIRM_OVERWRITE_END: &str = "? ";
pub const FB_HINT_DISKS: &str = " [D]disks ";
pub const FB_HINT_HIDDEN: &str = " [Ctrl+h]hidden ";
pub const FB_HINT_LOCATIONS: &str = " j/k:move  Enter:open  Esc:close ";
pub const MSG_CANNOT_DELETE_FILE: &str = "Cannot delete the currently open file";
pub const MSG_CANNOT_OVERWRITE_SECRET: &str = "Cannot overwrite a secret file";
pub const MSG_NAME_REQUIRED: &str = "Enter a file name first";
/// A directory read that did not answer within the listing timeout.
pub const MSG_DIRECTORY_NOT_RESPONDING: &str = "Directory not responding";
/// A file the user deleted is still listed: a sync client (or the OS) still
/// holds the name open, so the delete has not taken effect yet.
pub const MSG_DELETE_PENDING: &str = "Delete pending: a sync client still holds the file";
