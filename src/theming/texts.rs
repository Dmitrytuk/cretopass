pub const TITLE_TREE_DEFAULT: &str = " File ";
pub const TITLE_TREE_PREFIX: &str = " File — ";
pub const TITLE_SETTINGS_DEFAULT: &str = " Settings ";
pub const TITLE_SETTINGS_PREFIX: &str = " Settings — ";

pub const OVERLAY_SEARCH: &str = " Search ";
pub const OVERLAY_ADD_NODE: &str = "Add Node";
pub const OVERLAY_EDIT_NODE: &str = "Edit Node";
pub const OVERLAY_SELECT_FIELD_TYPE: &str = " Select Field Type ";
pub const OVERLAY_CUSTOM_FIELD_NAME: &str = " Custom Field Name ";
pub const OVERLAY_CONFIRM_DELETE: &str = " Confirm Delete ";
pub const OVERLAY_UNSAVED_CHANGES: &str = " Unsaved Changes ";
pub const OVERLAY_COPIED: &str = " Copied ";
pub const OVERLAY_HELP: &str = " Help ";

pub const SETTINGS_APP: &str = " Settings ";
pub const SETTINGS_CHANGE_PASSWORD: &str = " Change Password ";
pub const SETTINGS_NO_PASSPHRASE: &str = " No Passphrase ";
pub const SETTINGS_ENTER_PASSPHRASE: &str = " Enter Passphrase ";
pub const SETTINGS_ALERT: &str = " Alert ";
pub const ALERT_TITLE: &str = " Alert ";
pub const SETTINGS_DELETE_CONFIRM: &str = " Confirm Delete ";

pub const BROWSER_OPEN: &str = " Open File ";
pub const BROWSER_CREATE: &str = " Create New Secret File ";
pub const BROWSER_IMPORT: &str = " Import File ";

pub const MENU_ITEMS: &[&str] = &[
    "Open secret file",
    "Create new secret file",
    "Settings",
    "Change file password",
    "Export active file",
    "Import from file",
];

pub const SETTING_FULL_PATH: &str = "Display full path in title bar";
pub const SETTING_LAST_FILE: &str = "Start with last file";

pub const FIELD_TYPE_ITEMS: [&str; 3] = ["1. Login", "2. Password", "3. Custom Field"];

pub const EXPORT_SELECT_TITLE: &str = " Select Export Format ";
pub const EXPORT_CONFIRM_TITLE: &str = " Confirm Export ";
pub const EXPORT_TYPE_ITEMS: [&str; 3] = ["1. JSON", "2. XML", "3. CSV"];
pub const FORMAT_JSON: &str = "JSON";
pub const FORMAT_XML: &str = "XML";
pub const FORMAT_CSV: &str = "CSV";
pub const PROMPT_EXPORT_CONFIRM_START: &str = "Are you sure you want to export as ";
pub const PROMPT_EXPORT_CONFIRM_END: &str = "?";

pub const LABEL_NAME: &str = "Name: ";
pub const LABEL_DESC: &str = "Desc: ";
pub const LABEL_PASSPHRASE: &str = "Passphrase: ";
pub const LABEL_FILE: &str = "File: ";
pub const LABEL_CURRENT_PW: &str = "Current password";
pub const LABEL_NEW_PW: &str = "New password";
pub const LABEL_CONFIRM_PW: &str = "Confirm new password";
pub const LABEL_RENAME: &str = " Rename: ";
pub const LABEL_NAME_INLINE: &str = " Name: ";

pub const MODE_NORMAL: &str = "NORMAL";
pub const MODE_SEARCH: &str = "SEARCH";
pub const MODE_AWAIT_DD: &str = "AWAIT DD";
pub const MODE_DETAIL: &str = "DETAIL";
pub const MODE_EDIT: &str = "EDIT";
pub const MODE_ADD: &str = "ADD";
pub const MODE_FIELD_TYPE: &str = "FIELD TYPE";
pub const MODE_CUSTOM_NAME: &str = "CUSTOM NAME";
pub const MODE_CONFIRM: &str = "CONFIRM";
pub const MODE_DISCARD: &str = "DISCARD?";
pub const MODE_HELP: &str = "HELP";

pub const STATUS_LOADING: &str = "Loading...";
pub const STATUS_SAVING: &str = "Saving...";
pub const STATUS_SETTINGS_LABEL: &str = " SETTINGS ";

pub const HINT_NORMAL: &str =
    "j/k/h/l:nav  Enter:open  a/A:add  H:settings  /:search  dd:cut  p/P:paste  q:quit";
pub const HINT_SEARCH: &str = "Type to search  Enter:done  Esc:cancel";
pub const HINT_AWAIT_DD: &str = "Press d again to cut";
pub const HINT_DETAIL: &str = "j/k:navigate  Enter:copy  i:edit  Esc:close";
pub const HINT_DETAIL_ALERT: &str = "Press any key to continue";
pub const HINT_FORM: &str = "Tab:switch  Enter:save  Esc:cancel";
pub const HINT_FORM_EXTRA: &str = "Tab:switch  Del:delete  Enter:save  Esc:cancel";
pub const HINT_FORM_ADD: &str = "Tab:switch  Enter:add field  Esc:cancel";
pub const HINT_SELECT_FIELD_TYPE: &str = "j/k:nav  1/2/3:select  Enter:choose  Esc:cancel";
pub const HINT_CUSTOM_FIELD_NAME: &str = "Type name  Enter:done  Esc:cancel";
pub const HINT_CONFIRM_DELETE: &str = "y:delete  n/Esc:cancel";
pub const HINT_DISCARD: &str = "Enter/y:discard  n/Esc:continue editing";
pub const HINT_SETTINGS: &str = "j/k:navigate  Enter:select  d:<delete path> q:quit";
pub const HINT_APP_SETTINGS: &str = "Space:toggle  Enter:save  Esc:cancel";
pub const HINT_DELETE_CONFIRM: &str = "y/Enter:confirm  any key:cancel";
pub const HINT_CHANGE_PW_CONFIRM: &str = "y/Enter:yes  any key:no";
pub const HINT_CHANGE_PW_INPUT: &str = "Tab/↑/↓:switch  Enter:apply  Esc:cancel";
pub const HINT_NO_PASSPHRASE: &str = "y/Enter:yes  any key:no";
pub const HINT_PASSPHRASE: &str = "Enter:confirm  Esc:cancel";
pub const HINT_ALERT: &str = "Press any key to dismiss";
pub const HINT_HELP: &str = "Press any key to close";

pub const TREE_HELP_KEY_BINDINGS: &[&str] = &[
    " j/k      Navigate up/down",
    " h/l      Collapse/expand (leaf: l opens detail)",
    " Enter    Toggle expand (parent) or detail (leaf)",
    " i        Detail overlay for any node",
    " l        Open detail (leaf)",
    " d d      Cut selected node",
    " p / P    Paste as sibling / as child",
    " a        Add sibling (after selected)",
    " A        Add child",
    " /        Search",
    " ?        Show this help",
    " Esc      Clear search highlights",
    " q        Quit the app",
    " H        Return to settings menu",
];

pub const PROMPT_DELETE_FIELD_START: &str = "Delete \"";
pub const PROMPT_DELETE_FIELD_END: &str = "\" field? (y/N)";
pub const PROMPT_DELETE_RECENT_START: &str = "Remove '";
pub const PROMPT_DELETE_RECENT_END: &str = "' from recent files?";
pub const PROMPT_CHANGE_PW: &str = "Are you sure you want to change the password?";
pub const PROMPT_NO_PASSPHRASE: &str = "You are creating the secret file without password?";
pub const PROMPT_DISCARD: &str = "Discard changes?";

pub const OVERLAY_EXPORT_NODE: &str = " Export Node ";
pub const OVERLAY_EXPORT_FORMAT: &str = " Select Export Format ";
pub const EXPORT_NODE_OPTIONS: [&str; 2] =
    ["1. Export this node", "2. Export this node with children"];
pub const HINT_EXPORT_NODE_SELECT: &str = "j/k:nav  Enter:choose  Esc:cancel";
pub const HINT_EXPORT_NODE_FORMAT: &str = "j/k:nav  Enter:choose  Esc:cancel";
pub const MSG_EXPORT_NODE_SUCCESS: &str = "Node exported at ";
pub const MODE_EXPORT_SELECT: &str = "EXPORT";
pub const MODE_EXPORT_FORMAT: &str = "EXPORT FMT";

pub const MSG_NOT_IMPLEMENTED: &str = "This feature is not yet implemented";
pub const MSG_NO_OPEN_FILE: &str = "No file is currently open";
pub const MSG_CANNOT_DELETE_OPEN: &str = "Cannot delete the currently open file from recent files";
pub const MSG_CANNOT_WRITE: &str = "Cannot write to this path";
pub const MSG_WRONG_PW: &str = "wrong password: ";
pub const MSG_CURRENT_PW_INCORRECT: &str = "Current password is incorrect";
pub const MSG_PW_EMPTY: &str = "New password fields cannot be empty";
pub const MSG_PW_MISMATCH: &str = "New passwords do not match";
pub const MSG_SETTINGS_SAVED: &str = "Settings saved";
pub const MSG_SAVE_FAILED: &str = "Failed to save: ";
pub const MSG_EXPORT_SUCCESS: &str = "File exported at ";
pub const MSG_IMPORT_SUCCESS: &str = "File imported successfully.";
pub const MSG_IMPORT_NO_OPEN_FILE: &str = "Import requires an open file. Open one first.";
pub const MSG_NO_ACTIVE_FILE: &str = "No active file to derive export path";
pub const IMPORT_NAME_FALLBACK: &str = "unknown";
pub const MSG_NO_FILE_PATH: &str = "No file path set";

pub const EMPTY_DIR: &str = "  (empty directory)";
pub const NO_MATCHING_FILES: &str = "  No matching files";
pub const NO_DESCRIPTION: &str = "No description.";
pub const ADD_FIELD_BUTTON: &str = "+ Add Field";
pub const PASSWORD_MASK: &str = "*****";
pub const COPY_CHECK: &str = "✓ Copied!";
pub const SEPARATOR_RECENT_FILES: &str = "─── Recent Files ───";
pub const NEW_FOLDER_DEFAULT: &str = "New Folder";
pub const DEFAULT_NAME: &str = "Untitled";
pub const DEFAULT_ROOT_NAME: &str = "Secrets";
pub const CUSTOM_FIELD_DEFAULT: &str = "Custom";
pub const FIELD_LOGIN: &str = "Login";
pub const FIELD_PASSWORD: &str = "Password";
pub const FB_FILTER_ALL: &str = "All";
pub const FB_FILTER_PWDKEEPER: &str = "PwdKeeper";
pub const FB_HINT_FILTER: &str = " [f] ";
pub const FB_HINT_SEARCH: &str = " [/]search ";
pub const FB_HINT_NEW_DIR: &str = " [n]new dir ";
pub const FB_HINT_RENAME: &str = " [r]rename ";
pub const FB_HINT_DELETE: &str = " [d]delete ";
pub const FB_HINT_LIST: &str = " [N]list ";
pub const FB_HINT_NAME: &str = " [N]name ";
pub const FB_HINT_CONFIRM: &str = "Enter:confirm";
pub const FB_HINT_CANCEL: &str = "Esc:cancel";
pub const FB_DELETE_CONFIRM_SUFFIX: &str = " (y/N) ";
pub const MSG_CANNOT_DELETE_FILE: &str = "Cannot delete the currently open file";
