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
    ReturnToMenu,
    ToggleExpand,
    OpenDetail,
    EditNode,
    CopyField,
    Cut,
    PasteSibling,
    PasteChild,
    AddNode,
    AddSiblingNode,
    ToggleSearch,
    ConfirmSearch,
    ClearSearch,
    NextField,
    PrevField,
    InsertChar(char),
    Backspace,
    DeleteChar,
    SelectType1,
    SelectType2,
    SelectType3,
    SettingsDelete,
    ToggleSetting,
    SaveSettings,
    Yes,
    No,
    ExportNode,
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
            KeyCode::Char('p') => Self::PasteSibling,
            KeyCode::Char('P') => Self::PasteChild,
            KeyCode::Char('a') => Self::AddSiblingNode,
            KeyCode::Char('A') => Self::AddNode,
            KeyCode::Char('i') | KeyCode::Char('I') => Self::OpenDetail,
            KeyCode::Char('/') => Self::ToggleSearch,
            KeyCode::Char('q') | KeyCode::Char('Q') => Self::Quit,
            KeyCode::Char('H') => Self::ReturnToMenu,
            _ => Self::None,
        }
    }

    pub fn from_search(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Enter => Self::ConfirmSearch,
            KeyCode::Esc => Self::ClearSearch,
            KeyCode::Backspace | KeyCode::Delete => Self::Backspace,
            KeyCode::Char(c) => Self::InsertChar(c),
            _ => Self::None,
        }
    }

    pub fn from_form(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Tab | KeyCode::Down => Self::NextField,
            KeyCode::Up => Self::PrevField,
            KeyCode::Left => Self::CursorLeft,
            KeyCode::Right => Self::CursorRight,
            KeyCode::Home => Self::CursorHome,
            KeyCode::End => Self::CursorEnd,
            KeyCode::Backspace => Self::Backspace,
            KeyCode::Delete => Self::DeleteChar,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Esc => Self::Cancel,
            KeyCode::Char(c) => Self::InsertChar(c),
            _ => Self::None,
        }
    }

    pub fn from_detail(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Enter => Self::CopyField,
            KeyCode::Char('i') | KeyCode::Char('I') => Self::EditNode,
            KeyCode::Esc | KeyCode::Char(' ') => Self::Cancel,
            _ => Self::None,
        }
    }

    pub fn from_select_field_type(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Esc => Self::Cancel,
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Char('1') => Self::SelectType1,
            KeyCode::Char('2') => Self::SelectType2,
            KeyCode::Char('3') => Self::SelectType3,
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

    pub fn from_settings_menu(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => Self::NavDown,
            KeyCode::Char('k') | KeyCode::Up => Self::NavUp,
            KeyCode::Enter => Self::Confirm,
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => Self::Quit,
            KeyCode::Char('d') | KeyCode::Delete => Self::SettingsDelete,
            KeyCode::Char('H') => Self::ReturnToMenu,
            _ => Self::None,
        }
    }

    pub fn from_app_settings(key: &KeyEvent) -> Self {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => Self::NavDown,
            KeyCode::Up | KeyCode::Char('k') => Self::NavUp,
            KeyCode::Char(' ') => Self::ToggleSetting,
            KeyCode::Enter => Self::SaveSettings,
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
