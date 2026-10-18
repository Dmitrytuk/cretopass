use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
    /// Selects the item at the given index in a numbered list.
    SelectIndex(usize),
    ConfigDelete,
    ToggleSetting,
    SaveConfig,
    Yes,
    No,
    ExportNode,
    OpenHelp,
    None,
}

impl Action {
    pub fn from_normal(key: &KeyEvent) -> Self {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return match key.code {
                KeyCode::Enter => Self::OpenDetail,
                KeyCode::Char('c') => Self::Quit,
                KeyCode::Char('e') => Self::ExportNode,
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

    pub fn from_search(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Enter => Self::ConfirmSearch,
            KeyCode::Esc => Self::ClearSearch,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Char(c) => Self::InsertChar(c),
            _ => Self::None,
        }
    }

    /// Dispatch for the form's navigation level. Text is not editable here;
    /// `i`/`Enter` enter the input level, `j`/`k`/arrows/Tab move between fields.
    pub fn from_form_nav(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Tab => Self::NextField,
            KeyCode::Char('k') | KeyCode::Up | KeyCode::BackTab => Self::PrevField,
            KeyCode::Char('i') => Self::BeginInput,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Char('d') => Self::Cut,
            KeyCode::Char('p') => Self::PasteStart,
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
    /// field; `Esc` returns to the navigation level; `Enter` exits to Nav.
    pub fn from_form_input(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Enter => Self::ExitInput,
            KeyCode::Esc => Self::ExitInput,
            KeyCode::Char(c) => Self::InsertChar(c),
            _ => Self::None,
        }
    }

    pub fn from_detail(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Enter => Self::CopyField,
            KeyCode::Char('o') => Self::EditNode,
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
            KeyCode::Char(c) => Self::InsertChar(c),
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
            KeyCode::Enter => Self::Confirm,
            KeyCode::Char('q') | KeyCode::Char('Q') => Self::Quit,
            KeyCode::Esc => Self::Back,
            KeyCode::Char('d') | KeyCode::Delete => Self::ConfigDelete,
            KeyCode::Char('H') => Self::ReturnToMenu,
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
        match key.code {
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => Self::NextField,
            KeyCode::Up | KeyCode::Char('k') => Self::PrevField,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Esc => Self::Cancel,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Char(c) => Self::InsertChar(c),
            _ => Self::None,
        }
    }

    pub fn from_passphrase_input(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Esc => Self::Cancel,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Char(c) => Self::InsertChar(c),
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
    fn from_normal_tab_opens_detail() {
        assert_eq!(Action::from_normal(&key(KeyCode::Tab)), Action::OpenDetail);
        assert_eq!(
            Action::from_normal(&key(KeyCode::Char('i'))),
            Action::OpenDetail
        );
    }

    #[test]
    fn from_detail_o_edits_and_i_is_unbound() {
        assert_eq!(
            Action::from_detail(&key(KeyCode::Char('o'))),
            Action::EditNode
        );
        assert_eq!(Action::from_detail(&key(KeyCode::Char('i'))), Action::None);
        assert_eq!(Action::from_detail(&key(KeyCode::Char('I'))), Action::None);
    }

    #[test]
    fn from_detail_esc_cancels_and_space_is_unbound() {
        assert_eq!(Action::from_detail(&key(KeyCode::Esc)), Action::Cancel);
        assert_eq!(Action::from_detail(&key(KeyCode::Char(' '))), Action::None);
    }

    #[test]
    fn from_detail_enter_copies() {
        assert_eq!(Action::from_detail(&key(KeyCode::Enter)), Action::CopyField);
    }

    #[test]
    fn from_form_nav_jk_move_fields_and_i_enters_input() {
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
        assert_eq!(Action::from_form_nav(&key(KeyCode::Enter)), Action::Confirm);
        assert_eq!(Action::from_form_nav(&key(KeyCode::Esc)), Action::Cancel);
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
}
