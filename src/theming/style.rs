use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub enum ElementStyle {
    TreeBorder,
    TreeRow,
    TreeRowSelected,
    TreeRowMatched,
    ModeNormal,
    ModeSearch,
    ModeAwaitingDd,
    ModeDetail,
    ModeEdit,
    ModeAdd,
    ModeFieldType,
    ModeCustomName,
    ModeConfirmDelete,
    ModeDiscard,
    ModeHelp,
    StatusBarText,
    HelpBar,
    SearchBlock,
    EditBlock,
    FieldTypeBlock,
    CustomNameBlock,
    ConfirmDeleteBlock,
    CopiedBlock,
    DiscardBlock,
    DetailBlock,
    HelpBlock,
    HelpText,
    CopiedText,
    ConfirmMessage,
    DismissHint,
    SearchText,
    AddButton,
    AddButtonActive,
    InvertedCursor,
    GrayHint,
    StatusLoading,
    StatusSaving,
    StatusInfo,
    FbPathBar,
    FbDirName,
    FbAgeFileName,
    FbRegularFileName,
    FbSelectionOverlay,
    FbPwdMarker,
    FbPwdMarkerSelected,
    FbPrefix,
    FbEmptyText,
    FbHintText,
    FbActiveSearch,
    FbDeleteConfirm,
    FbDeleteHint,
    FbErrorBar,
    SettingsSelected,
    SettingsNormal,
    SettingsBlockBorder,
    SettingsStatusBar,
    SettingsHelp,
    SettingsSectionHeader,
    SettingsOverlayBlock,
    SettingsDeleteBlock,
    SettingsChangePwBlock,
    SettingsNoPassBlock,
    SettingsPassBlock,
    SettingsAlertBlock,
    SettingsErrorBar,
    SettingsPassLabel,
    SettingsFieldLabel,
    CursorLabel,
    CursorDimmed,
    CursorNormalText,
}

impl ElementStyle {
    pub fn style(self) -> Style {
        match self {
            Self::TreeBorder => Style::default().fg(Color::Cyan),
            Self::TreeRow => Style::default().fg(Color::White),
            Self::TreeRowSelected => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::TreeRowMatched => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::ModeNormal => Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeSearch => Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeAwaitingDd => Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeDetail => Style::default()
                .bg(Color::Magenta)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeEdit => Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            Self::ModeAdd => Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            Self::ModeFieldType => Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeCustomName => Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeConfirmDelete => Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeDiscard => Style::default()
                .bg(Color::Yellow)
                .fg(Color::Red)
                .add_modifier(Modifier::BOLD),
            Self::ModeHelp => Style::default()
                .bg(Color::Magenta)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::StatusBarText => Style::default().fg(Color::Gray),
            Self::HelpBar => Style::default().fg(Color::DarkGray),
            Self::SearchBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::EditBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::FieldTypeBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::CustomNameBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::ConfirmDeleteBlock => Style::default().bg(Color::Black).fg(Color::Red),
            Self::CopiedBlock => Style::default().bg(Color::Black).fg(Color::Green),
            Self::DiscardBlock => Style::default().bg(Color::Black).fg(Color::Yellow),
            Self::DetailBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::HelpBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::HelpText => Style::default().fg(Color::White),
            Self::CopiedText => Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Self::ConfirmMessage => Style::default().fg(Color::White),
            Self::DismissHint => Style::default().fg(Color::DarkGray),
            Self::SearchText => Style::default().fg(Color::White),
            Self::AddButton => Style::default().fg(Color::White),
            Self::AddButtonActive => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::InvertedCursor => Style::default().bg(Color::White).fg(Color::Black),
            Self::GrayHint => Style::default().fg(Color::DarkGray),
            Self::StatusLoading => Style::default().fg(Color::Green),
            Self::StatusSaving => Style::default().fg(Color::Red),
            Self::StatusInfo => Style::default().fg(Color::White),
            Self::FbPathBar => Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray),
            Self::FbDirName => Style::default().fg(Color::Blue),
            Self::FbAgeFileName => Style::default().fg(Color::Cyan),
            Self::FbRegularFileName => Style::default().fg(Color::White),
            Self::FbSelectionOverlay => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::FbPwdMarker => Style::default().fg(Color::Cyan),
            Self::FbPwdMarkerSelected => Style::default().fg(Color::White).bg(Color::DarkGray),
            Self::FbPrefix => Style::default().fg(Color::White),
            Self::FbEmptyText => Style::default().fg(Color::DarkGray),
            Self::FbHintText => Style::default().fg(Color::DarkGray),
            Self::FbActiveSearch => Style::default().fg(Color::Yellow),
            Self::FbDeleteConfirm => Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::FbDeleteHint => Style::default().bg(Color::Red).fg(Color::DarkGray),
            Self::FbErrorBar => Style::default().bg(Color::Red).fg(Color::White),
            Self::SettingsSelected => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::SettingsNormal => Style::default().fg(Color::White),
            Self::SettingsBlockBorder => Style::default().fg(Color::Cyan),
            Self::SettingsStatusBar => Style::default()
                .bg(Color::Cyan)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::SettingsHelp => Style::default().fg(Color::DarkGray),
            Self::SettingsSectionHeader => Style::default().fg(Color::DarkGray),
            Self::SettingsOverlayBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::SettingsDeleteBlock => Style::default().bg(Color::Black).fg(Color::Red),
            Self::SettingsChangePwBlock => Style::default().bg(Color::Black).fg(Color::Yellow),
            Self::SettingsNoPassBlock => Style::default().bg(Color::Black).fg(Color::Yellow),
            Self::SettingsPassBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::SettingsAlertBlock => Style::default().bg(Color::Black).fg(Color::Red),
            Self::SettingsErrorBar => Style::default().bg(Color::Red).fg(Color::White),
            Self::SettingsPassLabel => Style::default().fg(Color::Cyan),
            Self::SettingsFieldLabel => Style::default().fg(Color::White),
            Self::CursorLabel => Style::default().fg(Color::DarkGray),
            Self::CursorDimmed => Style::default().fg(Color::DarkGray),
            Self::CursorNormalText => Style::default().fg(Color::White),
        }
    }
}
