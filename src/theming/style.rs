use ratatui::style::{Modifier, Style};

use super::palette;

#[derive(Debug, Clone, Copy)]
pub enum ElementStyle {
    /// Base fill painted across the entire screen before anything else, so the
    /// terminal's own background never shows through (see
    /// [`paint_background`](super::paint_background)). Currently [`SURFACE`](palette::SURFACE).
    AppBackground,
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
    SearchText,
    /// Inline error inside the password-generator modal.
    GeneratorError,

    InvertedCursor,
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
    HomeSelected,
    HomeNormal,
    HomeBlockBorder,
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
            Self::AppBackground => Style::default().bg(palette::SURFACE),
            Self::TreeBorder => Style::default().fg(palette::ACCENT),
            Self::TreeRow => Style::default().fg(palette::TEXT),
            Self::ListRowSelected => Style::default().bg(palette::SELECTION_BG).fg(palette::TEXT),
            Self::TreeRowMatched => Style::default().bg(palette::SELECTION_BG).fg(palette::TEXT),
            Self::FormRowFocused => Style::default().bg(palette::SELECTION_BG).fg(palette::TEXT),
            Self::ModeNormal => Style::default()
                .bg(palette::SELECTION_BG)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::ModeSearch => Style::default()
                .bg(palette::INFO)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingCut => Style::default()
                .bg(palette::DANGER)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingPaste => Style::default()
                .bg(palette::WARNING)
                .fg(palette::INVERSE_FG)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingAdd => Style::default()
                .bg(palette::SUCCESS)
                .fg(palette::INVERSE_FG)
                .add_modifier(Modifier::BOLD),
            Self::ModeDetail => Style::default()
                .bg(palette::SELECTION_BG)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::ModeEdit => Style::default()
                .bg(palette::WARNING)
                .fg(palette::INVERSE_FG)
                .add_modifier(Modifier::BOLD),
            Self::ModeAdd => Style::default()
                .bg(palette::SUCCESS)
                .fg(palette::INVERSE_FG)
                .add_modifier(Modifier::BOLD),
            Self::ModeFieldType => Style::default()
                .bg(palette::INFO)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::ModeCustomName => Style::default()
                .bg(palette::INFO)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::ModeGenerator => Style::default()
                .bg(palette::ACCENT)
                .fg(palette::INVERSE_FG)
                .add_modifier(Modifier::BOLD),
            Self::ModeSymbols => Style::default()
                .bg(palette::ACCENT)
                .fg(palette::INVERSE_FG)
                .add_modifier(Modifier::BOLD),
            Self::ModeConfirmDelete => Style::default()
                .bg(palette::DANGER)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::ModeHelp => Style::default()
                .bg(palette::SELECTION_BG)
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD),
            Self::StatusBarText => Style::default().fg(palette::TEXT_MUTED),
            Self::SearchBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::EditBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::FieldTypeBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::CustomNameBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::GeneratorBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::SymbolsBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::ConfirmDeleteBlock => Style::default().bg(palette::SURFACE).fg(palette::DANGER),
            Self::CopiedBlock => Style::default().bg(palette::SURFACE).fg(palette::SUCCESS),
            Self::DetailBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::HelpBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::HelpText => Style::default().fg(palette::TEXT),
            Self::CopiedText => Style::default()
                .fg(palette::SUCCESS)
                .add_modifier(Modifier::BOLD),
            Self::ConfirmMessage => Style::default().fg(palette::TEXT),
            Self::SearchText => Style::default().fg(palette::TEXT),
            Self::GeneratorError => Style::default()
                .fg(palette::DANGER)
                .add_modifier(Modifier::BOLD),

            Self::InvertedCursor => Style::default()
                .bg(palette::INVERSE_BG)
                .fg(palette::INVERSE_FG),
            Self::StatusLoading => Style::default().fg(palette::STATUS_LOADING),
            Self::StatusSaving => Style::default().fg(palette::STATUS_SAVING),
            Self::StatusSaveFailed => Style::default().fg(palette::STATUS_FAILED),
            Self::StatusInfo => Style::default().fg(palette::TEXT),
            Self::FbPathBar => Style::default()
                .fg(palette::TEXT)
                .add_modifier(Modifier::BOLD)
                .bg(palette::PATH_BAR_BG),
            Self::FbDirName => Style::default().fg(palette::DIR_NAME),
            Self::FbAgeFileName => Style::default().fg(palette::ACCENT),
            Self::FbRegularFileName => Style::default().fg(palette::TEXT),
            Self::FbSelectionOverlay => {
                Style::default().bg(palette::SELECTION_BG).fg(palette::TEXT)
            }
            Self::FbPwdMarker => Style::default().fg(palette::ACCENT),
            Self::FbPwdMarkerSelected => {
                Style::default().fg(palette::TEXT).bg(palette::SELECTION_BG)
            }
            Self::FbPrefix => Style::default().fg(palette::TEXT),
            Self::FbEmptyText => Style::default().fg(palette::TEXT_FAINT),
            Self::FbHintText => Style::default().fg(palette::TEXT_FAINT),
            Self::HomeSelected => Style::default().bg(palette::SELECTION_BG).fg(palette::TEXT),
            Self::HomeNormal => Style::default().fg(palette::TEXT),
            Self::HomeBlockBorder => Style::default().fg(palette::ACCENT),
            Self::HomeSectionHeader => Style::default().fg(palette::TEXT_HEADER),
            Self::HomeOverlayBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::HomeDeleteBlock => Style::default().bg(palette::SURFACE).fg(palette::DANGER),
            Self::HomeChangePwBlock => Style::default().bg(palette::SURFACE).fg(palette::WARNING),
            Self::HomeNoPassBlock => Style::default().bg(palette::SURFACE).fg(palette::WARNING),
            Self::HomePassBlock => Style::default().bg(palette::SURFACE).fg(palette::ACCENT),
            Self::HomeAlertBlock => Style::default().bg(palette::SURFACE).fg(palette::DANGER),
            Self::HomeErrorBar => Style::default().bg(palette::DANGER).fg(palette::TEXT),
            Self::HomePassLabel => Style::default().fg(palette::ACCENT),
            Self::HomeFieldLabel => Style::default().fg(palette::TEXT),
            Self::CursorLabel => Style::default().fg(palette::CURSOR_DIM),
            Self::CursorDimmed => Style::default().fg(palette::CURSOR_DIM),
            Self::CursorNormalText => Style::default().fg(palette::TEXT),
        }
    }
}
