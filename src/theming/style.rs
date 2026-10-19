use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone, Copy)]
pub enum ElementStyle {
    TreeBorder,
    TreeRow,
    /// Selected row inside a list-style popup (Detail, field-type picker, …).
    ListRowSelected,
    TreeRowMatched,
    /// Focused row inside the Add/Edit form at the Nav level.
    FormRowFocused,
    ModeNormal,
    ModeSearch,
    ModePendingCut,
    ModePendingPaste,
    ModePendingAdd,
    ModeDetail,
    ModeEdit,
    ModeAdd,
    ModeFieldType,
    ModeCustomName,
    ModeGenerator,
    ModeSymbols,
    ModeConfirmDelete,
    ModeHelp,
    StatusBarText,
    HelpBar,
    SearchBlock,
    EditBlock,
    FieldTypeBlock,
    CustomNameBlock,
    GeneratorBlock,
    SymbolsBlock,
    ConfirmDeleteBlock,
    CopiedBlock,
    DetailBlock,
    HelpBlock,
    HelpText,
    CopiedText,
    ConfirmMessage,
    DismissHint,
    SearchText,
    /// Inline error inside the password-generator modal.
    GeneratorError,

    InvertedCursor,
    GrayHint,
    StatusLoading,
    StatusSaving,
    StatusSaveFailed,
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
    HomeSelected,
    HomeNormal,
    HomeBlockBorder,
    HomeStatusBar,
    HomeHelp,
    HomeSectionHeader,
    HomeOverlayBlock,
    HomeDeleteBlock,
    HomeChangePwBlock,
    HomeNoPassBlock,
    HomePassBlock,
    HomeAlertBlock,
    HomeErrorBar,
    HomePassLabel,
    HomeFieldLabel,
    CursorLabel,
    CursorDimmed,
    CursorNormalText,
}

impl ElementStyle {
    pub fn style(self) -> Style {
        match self {
            Self::TreeBorder => Style::default().fg(Color::Cyan),
            Self::TreeRow => Style::default().fg(Color::White),
            Self::ListRowSelected => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::TreeRowMatched => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::FormRowFocused => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::ModeNormal => Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModeSearch => Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingCut => Style::default()
                .bg(Color::Red)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingPaste => Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingAdd => Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
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
            Self::ModeGenerator => Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            Self::ModeSymbols => Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
            Self::ModeConfirmDelete => Style::default()
                .bg(Color::Red)
                .fg(Color::White)
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
            Self::GeneratorBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::SymbolsBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::ConfirmDeleteBlock => Style::default().bg(Color::Black).fg(Color::Red),
            Self::CopiedBlock => Style::default().bg(Color::Black).fg(Color::Green),
            Self::DetailBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::HelpBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::HelpText => Style::default().fg(Color::White),
            Self::CopiedText => Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            Self::ConfirmMessage => Style::default().fg(Color::White),
            Self::DismissHint => Style::default().fg(Color::DarkGray),
            Self::SearchText => Style::default().fg(Color::White),
            Self::GeneratorError => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),

            Self::InvertedCursor => Style::default().bg(Color::White).fg(Color::Black),
            Self::GrayHint => Style::default().fg(Color::DarkGray),
            Self::StatusLoading => Style::default().fg(Color::Green),
            Self::StatusSaving => Style::default().fg(Color::Yellow),
            Self::StatusSaveFailed => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
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
            Self::HomeSelected => Style::default().bg(Color::DarkGray).fg(Color::White),
            Self::HomeNormal => Style::default().fg(Color::White),
            Self::HomeBlockBorder => Style::default().fg(Color::Cyan),
            Self::HomeStatusBar => Style::default()
                .bg(Color::Cyan)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
            Self::HomeHelp => Style::default().fg(Color::DarkGray),
            Self::HomeSectionHeader => Style::default().fg(Color::DarkGray),
            Self::HomeOverlayBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::HomeDeleteBlock => Style::default().bg(Color::Black).fg(Color::Red),
            Self::HomeChangePwBlock => Style::default().bg(Color::Black).fg(Color::Yellow),
            Self::HomeNoPassBlock => Style::default().bg(Color::Black).fg(Color::Yellow),
            Self::HomePassBlock => Style::default().bg(Color::Black).fg(Color::Cyan),
            Self::HomeAlertBlock => Style::default().bg(Color::Black).fg(Color::Red),
            Self::HomeErrorBar => Style::default().bg(Color::Red).fg(Color::White),
            Self::HomePassLabel => Style::default().fg(Color::Cyan),
            Self::HomeFieldLabel => Style::default().fg(Color::White),
            Self::CursorLabel => Style::default().fg(Color::DarkGray),
            Self::CursorDimmed => Style::default().fg(Color::DarkGray),
            Self::CursorNormalText => Style::default().fg(Color::White),
        }
    }
}
