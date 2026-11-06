use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

/// Whether `key` is the `Ctrl+V` clipboard-paste chord.
///
/// Checked once per event loop, before any dispatcher, so the `v` can never
/// reach a plain-char match that would otherwise type the letter. `Ctrl+Shift+V`
/// counts too: a shifted `V` arrives uppercase, and terminals report their own
/// paste that way.
pub(crate) fn is_clipboard_paste(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char('v') | KeyCode::Char('V'))
}

/// Maps a mouse wheel tick to the navigation key it stands for.
///
/// A tick is a directional scroll with no modifiers, so it reuses the `Up`/`Down`
/// bindings every screen already has: the wheel and the arrows run the same
/// dispatch, with no per-screen coordinate math. Every other mouse event — a
/// click, a drag, a move — maps to `None`, because those are hit-tested against
/// a screen's own targets rather than translated into keys.
pub(crate) fn wheel_nav_key(mouse: &MouseEvent) -> Option<KeyEvent> {
    let code = match mouse.kind {
        MouseEventKind::ScrollUp => KeyCode::Up,
        MouseEventKind::ScrollDown => KeyCode::Down,
        _ => return None,
    };
    Some(KeyEvent::new(code, KeyModifiers::NONE))
}

/// Where a key press goes before it reaches any dispatcher.
///
/// Shared by every screen that reads its own keys (today the Tree and the Home
/// screen). Classifying once, up front, is what makes the invariant "`Ctrl+V`
/// never falls through to a `Char(c)` arm and types the letter `v`" structural:
/// the paste chord is checked first, so a new screen cannot silently regress it
/// by forgetting the check in its own dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyRoute {
    /// Read the system clipboard and paste it.
    Paste,
    /// Hand the key to the active modal.
    Modal,
    /// Hand the key to the screen's own dispatcher. The Tree calls it
    /// navigation; the Home screen calls it the menu — the same route.
    Screen,
}

/// Classifies a key press before it reaches any dispatcher.
///
/// The clipboard-paste chord is checked first, so `Ctrl+V` can never fall
/// through to a text field.
pub(crate) fn route_key(key: &KeyEvent, modal_open: bool) -> KeyRoute {
    if is_clipboard_paste(key) {
        KeyRoute::Paste
    } else if modal_open {
        KeyRoute::Modal
    } else {
        KeyRoute::Screen
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    NavDown,
    NavUp,
    NavLeft,
    NavRight,
    CursorLeft,
    CursorRight,
    CursorHome,
    CursorEnd,
    Confirm,
    Cancel,
    Quit,
    Back,
    ReturnToMenu,
    ToggleExpand,
    OpenDetail,
    EditNode,
    CopyField,
    Cut,
    PasteStart,
    PasteSibling,
    PasteChild,
    PasteBefore,
    AddStart,
    AddNode,
    AddSiblingNode,
    AddBefore,
    ToggleSearch,
    ConfirmSearch,
    ClearSearch,
    BeginInput,
    ExitInput,
    NextField,
    PrevField,
    InsertChar(char),
    Backspace,
    DeleteChar,
    /// Reverts the form's node to the previous history snapshot.
    Undo,
    /// Re-applies the next history snapshot after an undo.
    Redo,
    /// Selects the item at the given index in a numbered list.
    SelectIndex(usize),
    ConfigDelete,
    ToggleSetting,
    /// Toggles the focused item in the password-generator modal or its
    /// symbol checklist.
    Toggle,
    /// Generates a value for the focused field while editing it at the form's
    /// input level, when that field type has a generator (a `Login` word or a
    /// `Password`).
    Generate,
    SaveConfig,
    /// Toggles plaintext display of a password field.
    ToggleShowPassword,
    Yes,
    No,
    /// Export the selected Node to a plain-text file.
    ExportNode,
    /// Arms the `Ctrl+E` prefix: the next key picks Export (`e`) or Share (`s`).
    ExportStart,
    /// Share the selected Node's subtree as a new encrypted bundle.
    ShareNode,
    OpenHelp,
    None,
}

impl Action {
    /// An [`Action::InsertChar`] for `c`, or [`Action::None`] when `c` is a
    /// control character.
    ///
    /// `crossterm` can deliver a control character as `KeyCode::Char` — a C1
    /// control, or a paste on a terminal that ignored bracketed paste. A text
    /// field must never store one, so they are rejected at the dispatch
    /// boundary, before any insert path can see them.
    fn insert(c: char) -> Self {
        if c.is_control() {
            Self::None
        } else {
            Self::InsertChar(c)
        }
    }

    pub fn from_normal(key: &KeyEvent) -> Self {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Enter => Self::OpenDetail,
                KeyCode::Char('c') => Self::Quit,
                KeyCode::Char('e') => Self::ExportStart,
                _ => Self::None,
            };
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Char('h') | KeyCode::Left => Self::NavLeft,
            KeyCode::Char('l') | KeyCode::Right => Self::NavRight,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Esc => Self::Cancel,
            KeyCode::Char(' ') => Self::ToggleExpand,
            KeyCode::Char('d') => Self::Cut,
            KeyCode::Char('p') => Self::PasteStart,
            KeyCode::Char('a') => Self::AddStart,
            KeyCode::Char('i') => Self::OpenDetail,
            KeyCode::Tab => Self::OpenDetail,
            KeyCode::Char('/') => Self::ToggleSearch,
            KeyCode::Char('q') => Self::Quit,
            KeyCode::Char('H') => Self::ReturnToMenu,
            KeyCode::Char('?') => Self::OpenHelp,
            _ => Self::None,
        }
    }

    /// Dispatch for the `PendingPaste` mode entered by the first `p` press.
    pub fn from_pending_paste(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('p') => Self::PasteSibling,
            KeyCode::Char('P') => Self::PasteBefore,
            KeyCode::Char('l') | KeyCode::Right => Self::PasteChild,
            _ => Self::Cancel,
        }
    }

    /// Dispatch for the `PendingAdd` mode entered by the first `a` press.
    pub fn from_pending_add(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('a') => Self::AddSiblingNode,
            KeyCode::Char('A') => Self::AddBefore,
            KeyCode::Char('l') | KeyCode::Right => Self::AddNode,
            _ => Self::Cancel,
        }
    }

    /// Dispatch for the `PendingCut` mode entered by the first `d` press.
    pub fn from_pending_cut(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('d') => Self::Cut,
            _ => Self::Cancel,
        }
    }

    /// Dispatch for the `PendingExport` mode entered by `Ctrl+E`.
    ///
    /// `e` keeps the single-node export, `s` starts the share flow; every other
    /// key cancels, exactly like the `d`/`a`/`p` prefixes.
    pub fn from_pending_export(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('e') => Self::ExportNode,
            KeyCode::Char('s') => Self::ShareNode,
            _ => Self::Cancel,
        }
    }

    pub fn from_search(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Enter => Self::ConfirmSearch,
            KeyCode::Esc => Self::ClearSearch,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Char(c) => Self::insert(c),
            _ => Self::None,
        }
    }

    /// Dispatch for the form's navigation level. Text is not editable here;
    /// `i`/`Enter` enter the input level, `j`/`k`/arrows/Tab move between
    /// fields, `u`/`r` undo/redo, `Esc` closes the form.
    ///
    /// The field generator is invoked from the input level instead (see
    /// [`Action::from_form_input`]); `Ctrl+G` here is unbound.
    pub fn from_form_nav(key: &KeyEvent) -> Self {
        // No Ctrl chord is bound at the Nav level. The plain-char matches below
        // deliberately ignore modifiers (Ctrl+u/r/d/p/i/j keep their existing
        // meanings), so `Ctrl+G` falls through to the unbound `g`.
        match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => Self::NextField,
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => Self::PrevField,
            KeyCode::Char('i') | KeyCode::Enter => Self::BeginInput,
            KeyCode::Char('u') => Self::Undo,
            KeyCode::Char('r') => Self::Redo,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Char('d') => Self::Cut,
            KeyCode::Char('p') => Self::PasteStart,
            KeyCode::Esc => Self::Cancel,
            _ => Self::None,
        }
    }

    /// Dispatch for the password-generator parameter modal.
    ///
    /// Rows are navigated with `j`/`k`/arrows/`Tab`; `Space` toggles all
    /// symbols; `Enter` acts on the focused row (advancing off the length row);
    /// digits edit the length and `Left`/`Right`/`Home`/`End`/`Delete` move its
    /// caret (the handler ignores them unless the length row is focused);
    /// `Esc` cancels.
    pub fn from_generator(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => Self::NavUp,
            KeyCode::Char(' ') => Self::Toggle,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Esc => Self::Cancel,
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Char(c) => Self::insert(c),
            _ => Self::None,
        }
    }

    /// Dispatch for the per-symbol checklist opened from the generator modal.
    ///
    /// The checklist is a three-column grid: `h`/`l` (and Left/Right) move by
    /// one cell, `j`/`k` (and Down/Up) move by one row. `Space`/`Enter` toggle
    /// the focused symbol; `Esc` returns to the parameter modal.
    pub fn from_symbol_checklist(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => Self::NavLeft,
            KeyCode::Char('l') | KeyCode::Right => Self::NavRight,
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Char(' ') | KeyCode::Enter => Self::Toggle,
            KeyCode::Esc => Self::Cancel,
            _ => Self::None,
        }
    }

    /// Dispatch while the form holds a pending field cut (`Cut`) or paste
    /// (`Paste`) prefix. Mirrors the tree's `PendingCut` / `PendingPaste`.
    pub fn from_form_pending(key: &KeyEvent, paste: bool) -> Self {
        match (paste, key.code) {
            (false, KeyCode::Char('d')) => Self::Cut,
            (true, KeyCode::Char('p')) => Self::PasteSibling,
            (true, KeyCode::Char('P')) => Self::PasteBefore,
            _ => Self::Cancel,
        }
    }

    /// Dispatch for the form's input level. Printable keys edit the active
    /// field; both `Enter` and `Esc` return to the navigation level, committing
    /// the field's text.
    ///
    /// `Ctrl+G` invokes the field generator for the focused field when it has
    /// one (today: a `Login` word or a `Password`); elsewhere it is a no-op. It
    /// must be checked *before* the plain-char match, which ignores modifiers.
    pub fn from_form_input(key: &KeyEvent) -> Self {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('g') {
            return Self::Generate;
        }
        match key.code {
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Enter => Self::ExitInput,
            KeyCode::Esc => Self::ExitInput,
            KeyCode::Char(c) => Self::insert(c),
            _ => Self::None,
        }
    }

    pub fn from_detail(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Tab => Self::NextField,
            KeyCode::BackTab => Self::PrevField,
            KeyCode::Enter => Self::EditNode,
            KeyCode::Char('i') => Self::EditNode,
            KeyCode::Char('c') | KeyCode::Char('y') => Self::CopyField,
            KeyCode::Esc => Self::Cancel,
            _ => Self::None,
        }
    }

    pub fn from_select_field_type(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Esc => Self::Cancel,
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Char(c @ '1'..='9') => {
                let index = (c as u8 - b'1') as usize;
                Self::SelectIndex(index)
            }
            KeyCode::Enter => Self::Confirm,
            _ => Self::None,
        }
    }

    pub fn from_custom_field_name(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Esc => Self::Cancel,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Backspace | KeyCode::Delete => Self::Backspace,
            KeyCode::Char(c) => Self::insert(c),
            _ => Self::None,
        }
    }

    pub fn from_confirm(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => Self::Yes,
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => Self::No,
            _ => Self::None,
        }
    }

    pub fn from_home(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => Self::Confirm,
            KeyCode::Char('q') | KeyCode::Char('Q') => Self::Quit,
            KeyCode::Esc => Self::Back,
            KeyCode::Char('d') | KeyCode::Delete => Self::ConfigDelete,
            KeyCode::Char('H') => Self::ReturnToMenu,
            KeyCode::Char('?') => Self::OpenHelp,
            _ => Self::None,
        }
    }

    pub fn from_config(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => Self::NavDown,
            KeyCode::Up | KeyCode::Char('k') => Self::NavUp,
            KeyCode::Char(' ') => Self::ToggleSetting,
            KeyCode::Enter => Self::SaveConfig,
            KeyCode::Esc => Self::Cancel,
            _ => Self::None,
        }
    }

    pub fn from_change_pw_input(key: &KeyEvent) -> Self {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('s') => Self::ToggleShowPassword,
                _ => Self::None,
            };
        }
        match key.code {
            KeyCode::Tab | KeyCode::Down => Self::NextField,
            KeyCode::BackTab | KeyCode::Up => Self::PrevField,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Esc => Self::Cancel,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Char(c) => Self::insert(c),
            _ => Self::None,
        }
    }

    /// Dispatch for the dedicated "create new secret file" dialog. Password
    /// fields are focused, so `j`/`k` are typed as text and field navigation
    /// uses `Tab`/arrows. `Ctrl+S` reveals the passwords.
    pub fn from_create_secret_file(key: &KeyEvent) -> Self {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('s') => Self::ToggleShowPassword,
                _ => Self::None,
            };
        }
        match key.code {
            KeyCode::Tab | KeyCode::Down => Self::NextField,
            KeyCode::BackTab | KeyCode::Up => Self::PrevField,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Esc => Self::Cancel,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Char(c) => Self::insert(c),
            _ => Self::None,
        }
    }

    pub fn from_passphrase_input(key: &KeyEvent) -> Self {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Char('s') => Self::ToggleShowPassword,
                _ => Self::None,
            };
        }
        match key.code {
            KeyCode::Esc => Self::Cancel,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Char(c) => Self::insert(c),
            _ => Self::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn from_search_delete_is_none() {
        assert_eq!(Action::from_search(&key(KeyCode::Delete)), Action::None);
    }

    #[test]
    fn from_search_backspace_is_backspace() {
        assert_eq!(
            Action::from_search(&key(KeyCode::Backspace)),
            Action::Backspace
        );
    }

    #[test]
    fn from_search_printable_inserts() {
        assert_eq!(
            Action::from_search(&key(KeyCode::Char('x'))),
            Action::InsertChar('x')
        );
    }

    #[test]
    fn from_search_enter_confirms_and_esc_clears() {
        assert_eq!(
            Action::from_search(&key(KeyCode::Enter)),
            Action::ConfirmSearch
        );
        assert_eq!(Action::from_search(&key(KeyCode::Esc)), Action::ClearSearch);
    }

    #[test]
    fn from_pending_cut_second_d_cuts() {
        assert_eq!(
            Action::from_pending_cut(&key(KeyCode::Char('d'))),
            Action::Cut
        );
    }

    #[test]
    fn from_pending_cut_any_other_key_cancels() {
        assert_eq!(
            Action::from_pending_cut(&key(KeyCode::Char('x'))),
            Action::Cancel
        );
        assert_eq!(Action::from_pending_cut(&key(KeyCode::Esc)), Action::Cancel);
        assert_eq!(
            Action::from_pending_cut(&key(KeyCode::Enter)),
            Action::Cancel
        );
    }

    #[test]
    fn from_pending_export_e_exports_and_s_shares() {
        assert_eq!(
            Action::from_pending_export(&key(KeyCode::Char('e'))),
            Action::ExportNode
        );
        assert_eq!(
            Action::from_pending_export(&key(KeyCode::Char('s'))),
            Action::ShareNode
        );
    }

    #[test]
    fn from_pending_export_any_other_key_cancels() {
        assert_eq!(
            Action::from_pending_export(&key(KeyCode::Char('x'))),
            Action::Cancel
        );
        assert_eq!(
            Action::from_pending_export(&key(KeyCode::Esc)),
            Action::Cancel
        );
        assert_eq!(
            Action::from_pending_export(&key(KeyCode::Enter)),
            Action::Cancel
        );
    }

    #[test]
    fn ctrl_e_arms_the_export_share_prefix() {
        assert_eq!(
            Action::from_normal(&ctrl(KeyCode::Char('e'))),
            Action::ExportStart
        );
    }

    #[test]
    fn from_normal_tab_opens_detail() {
        assert_eq!(Action::from_normal(&key(KeyCode::Tab)), Action::OpenDetail);
        assert_eq!(
            Action::from_normal(&key(KeyCode::Char('i'))),
            Action::OpenDetail
        );
    }

    #[test]
    fn from_detail_i_and_enter_edit_and_c_y_copy() {
        assert_eq!(
            Action::from_detail(&key(KeyCode::Char('i'))),
            Action::EditNode
        );
        assert_eq!(Action::from_detail(&key(KeyCode::Enter)), Action::EditNode);
        assert_eq!(
            Action::from_detail(&key(KeyCode::Char('c'))),
            Action::CopyField
        );
        assert_eq!(
            Action::from_detail(&key(KeyCode::Char('y'))),
            Action::CopyField
        );
    }

    #[test]
    fn from_detail_tab_navigates_fields() {
        assert_eq!(Action::from_detail(&key(KeyCode::Tab)), Action::NextField);
        assert_eq!(
            Action::from_detail(&key(KeyCode::BackTab)),
            Action::PrevField
        );
    }

    #[test]
    fn from_detail_esc_cancels_and_space_is_unbound() {
        assert_eq!(Action::from_detail(&key(KeyCode::Esc)), Action::Cancel);
        assert_eq!(Action::from_detail(&key(KeyCode::Char(' '))), Action::None);
    }

    #[test]
    fn from_detail_o_is_unbound() {
        assert_eq!(Action::from_detail(&key(KeyCode::Char('o'))), Action::None);
        assert_eq!(Action::from_detail(&key(KeyCode::Char('O'))), Action::None);
    }

    #[test]
    fn from_form_nav_jk_move_fields_and_i_enter_begin_input() {
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('j'))),
            Action::NextField
        );
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Down)),
            Action::NextField
        );
        assert_eq!(Action::from_form_nav(&key(KeyCode::Tab)), Action::NextField);
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('k'))),
            Action::PrevField
        );
        assert_eq!(Action::from_form_nav(&key(KeyCode::Up)), Action::PrevField);
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::BackTab)),
            Action::PrevField
        );
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('i'))),
            Action::BeginInput
        );
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Enter)),
            Action::BeginInput
        );
        assert_eq!(Action::from_form_nav(&key(KeyCode::Esc)), Action::Cancel);
    }

    #[test]
    fn from_form_nav_binds_undo_and_redo() {
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('u'))),
            Action::Undo
        );
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('r'))),
            Action::Redo
        );
    }

    #[test]
    fn from_form_nav_does_not_insert_chars() {
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('x'))),
            Action::None
        );
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('5'))),
            Action::None
        );
    }

    #[test]
    fn from_form_input_inserts_and_enter_esc_exit() {
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Char('x'))),
            Action::InsertChar('x')
        );
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Char('j'))),
            Action::InsertChar('j')
        );
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Esc)),
            Action::ExitInput
        );
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Enter)),
            Action::ExitInput
        );
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Left)),
            Action::CursorLeft
        );
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Backspace)),
            Action::Backspace
        );
    }

    #[test]
    fn from_form_nav_binds_field_cut_and_paste() {
        assert_eq!(Action::from_form_nav(&key(KeyCode::Char('d'))), Action::Cut);
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('p'))),
            Action::PasteStart
        );
    }

    #[test]
    fn from_form_pending_cut_requires_second_d() {
        assert_eq!(
            Action::from_form_pending(&key(KeyCode::Char('d')), false),
            Action::Cut
        );
        assert_eq!(
            Action::from_form_pending(&key(KeyCode::Char('p')), false),
            Action::Cancel
        );
        assert_eq!(
            Action::from_form_pending(&key(KeyCode::Esc), false),
            Action::Cancel
        );
    }

    #[test]
    fn from_form_pending_paste_chooses_direction() {
        assert_eq!(
            Action::from_form_pending(&key(KeyCode::Char('p')), true),
            Action::PasteSibling
        );
        assert_eq!(
            Action::from_form_pending(&key(KeyCode::Char('P')), true),
            Action::PasteBefore
        );
        assert_eq!(
            Action::from_form_pending(&key(KeyCode::Char('d')), true),
            Action::Cancel
        );
    }

    #[test]
    fn from_form_input_does_not_navigate_fields() {
        assert_eq!(Action::from_form_input(&key(KeyCode::Tab)), Action::None);
        assert_eq!(Action::from_form_input(&key(KeyCode::Down)), Action::None);
        assert_eq!(Action::from_form_input(&key(KeyCode::Up)), Action::None);
    }

    #[test]
    fn from_home_l_and_right_open_selected_item() {
        assert_eq!(Action::from_home(&key(KeyCode::Char('l'))), Action::Confirm);
        assert_eq!(Action::from_home(&key(KeyCode::Right)), Action::Confirm);
        assert_eq!(Action::from_home(&key(KeyCode::Enter)), Action::Confirm);
    }

    #[test]
    fn from_home_question_opens_about() {
        assert_eq!(
            Action::from_home(&key(KeyCode::Char('?'))),
            Action::OpenHelp
        );
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn route_key_checks_the_paste_chord_before_the_modal() {
        // The chord wins whatever the screen state, so a paste can never fall
        // through to a `Char(c)` arm; without it, the modal decides.
        assert_eq!(route_key(&ctrl(KeyCode::Char('v')), true), KeyRoute::Paste);
        assert_eq!(route_key(&ctrl(KeyCode::Char('v')), false), KeyRoute::Paste);
        assert_eq!(route_key(&key(KeyCode::Char('v')), true), KeyRoute::Modal);
        assert_eq!(route_key(&key(KeyCode::Char('v')), false), KeyRoute::Screen);
    }

    #[test]
    fn ctrl_v_is_the_clipboard_paste_chord() {
        assert!(is_clipboard_paste(&ctrl(KeyCode::Char('v'))));
        assert!(
            is_clipboard_paste(&KeyEvent::new(
                KeyCode::Char('V'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            "a shifted Ctrl+V is still the paste chord"
        );
        assert!(
            !is_clipboard_paste(&key(KeyCode::Char('v'))),
            "a bare `v` must keep typing"
        );
        assert!(
            !is_clipboard_paste(&ctrl(KeyCode::Char('c'))),
            "only `v` pastes"
        );
    }

    fn mouse(kind: MouseEventKind) -> MouseEvent {
        MouseEvent {
            kind,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        }
    }

    #[test]
    fn wheel_ticks_map_to_the_arrow_keys() {
        assert_eq!(
            wheel_nav_key(&mouse(MouseEventKind::ScrollUp)).map(|k| k.code),
            Some(KeyCode::Up)
        );
        assert_eq!(
            wheel_nav_key(&mouse(MouseEventKind::ScrollDown)).map(|k| k.code),
            Some(KeyCode::Down)
        );
    }

    #[test]
    fn clicks_and_moves_are_not_navigation_keys() {
        assert_eq!(wheel_nav_key(&mouse(MouseEventKind::Moved)), None);
        assert_eq!(
            wheel_nav_key(&mouse(MouseEventKind::Down(
                crossterm::event::MouseButton::Left
            ))),
            None
        );
    }

    #[test]
    fn ctrl_s_toggles_show_password() {
        assert_eq!(
            Action::from_passphrase_input(&ctrl(KeyCode::Char('s'))),
            Action::ToggleShowPassword
        );
        assert_eq!(
            Action::from_change_pw_input(&ctrl(KeyCode::Char('s'))),
            Action::ToggleShowPassword
        );
        assert_eq!(
            Action::from_create_secret_file(&ctrl(KeyCode::Char('s'))),
            Action::ToggleShowPassword
        );
    }

    #[test]
    fn create_secret_file_navigates_with_tab_and_arrows() {
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Tab)),
            Action::NextField
        );
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::BackTab)),
            Action::PrevField
        );
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Down)),
            Action::NextField
        );
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Up)),
            Action::PrevField
        );
    }

    #[test]
    fn create_secret_file_typing_j_k_inserts_text() {
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Char('j'))),
            Action::InsertChar('j')
        );
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Char('k'))),
            Action::InsertChar('k')
        );
    }

    #[test]
    fn create_secret_file_enter_confirms_and_esc_cancels() {
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Enter)),
            Action::Confirm
        );
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Esc)),
            Action::Cancel
        );
    }

    #[test]
    fn change_pw_input_typing_j_k_inserts_text() {
        assert_eq!(
            Action::from_change_pw_input(&key(KeyCode::Char('j'))),
            Action::InsertChar('j')
        );
        assert_eq!(
            Action::from_change_pw_input(&key(KeyCode::Char('k'))),
            Action::InsertChar('k')
        );
        assert_eq!(
            Action::from_change_pw_input(&key(KeyCode::Tab)),
            Action::NextField
        );
        assert_eq!(
            Action::from_change_pw_input(&key(KeyCode::BackTab)),
            Action::PrevField
        );
    }

    #[test]
    fn from_form_nav_ctrl_g_is_unbound_and_plain_g_is_unbound() {
        assert_eq!(
            Action::from_form_nav(&ctrl(KeyCode::Char('g'))),
            Action::None
        );
        assert_eq!(
            Action::from_form_nav(&key(KeyCode::Char('g'))),
            Action::None
        );
    }

    #[test]
    fn from_form_nav_ctrl_chords_keep_their_existing_meanings() {
        assert_eq!(
            Action::from_form_nav(&ctrl(KeyCode::Char('u'))),
            Action::Undo
        );
        assert_eq!(
            Action::from_form_nav(&ctrl(KeyCode::Char('r'))),
            Action::Redo
        );
        assert_eq!(
            Action::from_form_nav(&ctrl(KeyCode::Char('d'))),
            Action::Cut
        );
        assert_eq!(
            Action::from_form_nav(&ctrl(KeyCode::Char('p'))),
            Action::PasteStart
        );
        assert_eq!(
            Action::from_form_nav(&ctrl(KeyCode::Char('i'))),
            Action::BeginInput
        );
        assert_eq!(
            Action::from_form_nav(&ctrl(KeyCode::Char('j'))),
            Action::NextField
        );
    }

    #[test]
    fn from_form_input_ctrl_g_invokes_the_generator_and_plain_g_inserts() {
        // The generator is invoked from the input level; on a field without one
        // the handler treats the action as a no-op.
        assert_eq!(
            Action::from_form_input(&ctrl(KeyCode::Char('g'))),
            Action::Generate
        );
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Char('g'))),
            Action::InsertChar('g')
        );
    }

    #[test]
    fn from_generator_navigates_toggles_and_edits_length() {
        assert_eq!(
            Action::from_generator(&key(KeyCode::Char('j'))),
            Action::NavDown
        );
        assert_eq!(Action::from_generator(&key(KeyCode::Tab)), Action::NavDown);
        assert_eq!(
            Action::from_generator(&key(KeyCode::Char('k'))),
            Action::NavUp
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::BackTab)),
            Action::NavUp
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Char(' '))),
            Action::Toggle
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Enter)),
            Action::Confirm
        );
        assert_eq!(Action::from_generator(&key(KeyCode::Esc)), Action::Cancel);
        assert_eq!(
            Action::from_generator(&key(KeyCode::Char('5'))),
            Action::InsertChar('5')
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Backspace)),
            Action::Backspace
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Left)),
            Action::CursorLeft
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Right)),
            Action::CursorRight
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Home)),
            Action::CursorHome
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::End)),
            Action::CursorEnd
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Delete)),
            Action::DeleteChar
        );
    }

    #[test]
    fn from_symbol_checklist_is_a_grid_with_toggle_and_escape() {
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Char('h'))),
            Action::NavLeft
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Left)),
            Action::NavLeft
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Char('l'))),
            Action::NavRight
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Right)),
            Action::NavRight
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Char('j'))),
            Action::NavDown
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Char('k'))),
            Action::NavUp
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Char(' '))),
            Action::Toggle
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Enter)),
            Action::Toggle
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Esc)),
            Action::Cancel
        );
        assert_eq!(
            Action::from_symbol_checklist(&key(KeyCode::Char('x'))),
            Action::None
        );
    }

    #[test]
    fn a_control_char_key_never_inserts() {
        // `crossterm` can deliver a control character as `KeyCode::Char` (a C1
        // control, or a paste on a terminal that ignored bracketed paste); a
        // text field must reject it rather than store it.
        assert_eq!(
            Action::from_search(&key(KeyCode::Char('\u{9b}'))),
            Action::None
        );
        assert_eq!(
            Action::from_form_input(&key(KeyCode::Char('\u{1b}'))),
            Action::None
        );
        assert_eq!(
            Action::from_create_secret_file(&key(KeyCode::Char('\u{7}'))),
            Action::None
        );
        assert_eq!(
            Action::from_custom_field_name(&key(KeyCode::Char('\u{9b}'))),
            Action::None
        );
        assert_eq!(
            Action::from_generator(&key(KeyCode::Char('\u{1b}'))),
            Action::None
        );
        assert_eq!(
            Action::from_change_pw_input(&key(KeyCode::Char('\u{9b}'))),
            Action::None
        );
    }
}
