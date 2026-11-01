use ratatui::style::{Modifier, Style};

use super::palette;

#[derive(Debug, Clone, Copy)]
pub enum ElementStyle {
    /// Base fill painted across the entire screen before anything else, so the
    /// terminal's own background never shows through (see
    /// [`paint_background`](super::paint_background)). The canvas is the active
    /// scheme's `base_bg`; the `surface` panels sit on it, above or below it
    /// depending on the scheme.
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
    ModePendingExport,
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
    /// The alert card, coloured by outcome: success green, error red.
    AlertSuccessBlock,
    AlertErrorBlock,
    DetailBlock,
    HelpBlock,
    HelpText,
    AlertSuccessText,
    AlertErrorText,
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
    /// The file name an export will write, appended to the browser's path bar.
    FbExportName,
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
    HomeErrorBar,
    HomePassLabel,
    HomeFieldLabel,
    CursorLabel,
    CursorDimmed,
    CursorNormalText,
}

impl ElementStyle {
    pub fn style(self) -> Style {
        let scheme = palette();
        match self {
            Self::AppBackground => Style::default().bg(scheme.base_bg),
            Self::TreeBorder => Style::default().fg(scheme.accent),
            Self::TreeRow => Style::default().fg(scheme.text),
            Self::ListRowSelected => Style::default().bg(scheme.selection_bg).fg(scheme.text),
            Self::TreeRowMatched => Style::default().bg(scheme.selection_bg).fg(scheme.text),
            Self::FormRowFocused => Style::default().bg(scheme.selection_bg).fg(scheme.text),
            Self::ModeNormal => Style::default()
                .bg(scheme.selection_bg)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModeSearch => Style::default()
                .bg(scheme.info)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingCut => Style::default()
                .bg(scheme.danger)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingPaste => Style::default()
                .bg(scheme.warning)
                .fg(scheme.inverse_fg)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingAdd => Style::default()
                .bg(scheme.success)
                .fg(scheme.inverse_fg)
                .add_modifier(Modifier::BOLD),
            Self::ModePendingExport => Style::default()
                .bg(scheme.info)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModeDetail => Style::default()
                .bg(scheme.selection_bg)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModeEdit => Style::default()
                .bg(scheme.warning)
                .fg(scheme.inverse_fg)
                .add_modifier(Modifier::BOLD),
            Self::ModeAdd => Style::default()
                .bg(scheme.success)
                .fg(scheme.inverse_fg)
                .add_modifier(Modifier::BOLD),
            Self::ModeFieldType => Style::default()
                .bg(scheme.info)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModeCustomName => Style::default()
                .bg(scheme.info)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModeGenerator => Style::default()
                .bg(scheme.accent)
                .fg(scheme.inverse_fg)
                .add_modifier(Modifier::BOLD),
            Self::ModeSymbols => Style::default()
                .bg(scheme.accent)
                .fg(scheme.inverse_fg)
                .add_modifier(Modifier::BOLD),
            Self::ModeConfirmDelete => Style::default()
                .bg(scheme.danger)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::ModeHelp => Style::default()
                .bg(scheme.selection_bg)
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD),
            Self::StatusBarText => Style::default().fg(scheme.text_muted),
            Self::SearchBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::EditBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::FieldTypeBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::CustomNameBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::GeneratorBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::SymbolsBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::ConfirmDeleteBlock => Style::default().bg(scheme.surface).fg(scheme.danger),
            Self::AlertSuccessBlock => Style::default().bg(scheme.surface).fg(scheme.success),
            Self::AlertErrorBlock => Style::default().bg(scheme.surface).fg(scheme.danger),
            Self::DetailBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::HelpBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::HelpText => Style::default().fg(scheme.text),
            Self::AlertSuccessText => Style::default()
                .fg(scheme.success)
                .add_modifier(Modifier::BOLD),
            Self::AlertErrorText => Style::default().fg(scheme.text),
            Self::ConfirmMessage => Style::default().fg(scheme.text),
            Self::SearchText => Style::default().fg(scheme.text),
            Self::GeneratorError => Style::default()
                .fg(scheme.danger)
                .add_modifier(Modifier::BOLD),

            Self::InvertedCursor => Style::default().bg(scheme.inverse_bg).fg(scheme.inverse_fg),
            Self::StatusLoading => Style::default().fg(scheme.status_loading),
            Self::StatusSaving => Style::default().fg(scheme.status_saving),
            Self::StatusSaveFailed => Style::default().fg(scheme.status_failed),
            Self::StatusInfo => Style::default().fg(scheme.text),
            Self::FbPathBar => Style::default()
                .fg(scheme.text)
                .add_modifier(Modifier::BOLD)
                .bg(scheme.path_bar_bg),
            Self::FbExportName => Style::default()
                .fg(scheme.accent)
                .add_modifier(Modifier::BOLD)
                .bg(scheme.path_bar_bg),
            Self::FbDirName => Style::default().fg(scheme.dir_name),
            Self::FbAgeFileName => Style::default().fg(scheme.accent),
            Self::FbRegularFileName => Style::default().fg(scheme.text),
            Self::FbSelectionOverlay => Style::default().bg(scheme.selection_bg).fg(scheme.text),
            Self::FbPwdMarker => Style::default().fg(scheme.accent),
            Self::FbPwdMarkerSelected => Style::default().fg(scheme.text).bg(scheme.selection_bg),
            Self::FbPrefix => Style::default().fg(scheme.text),
            Self::FbEmptyText => Style::default().fg(scheme.text_faint),
            Self::FbHintText => Style::default().fg(scheme.text_faint),
            Self::HomeSelected => Style::default().bg(scheme.selection_bg).fg(scheme.text),
            Self::HomeNormal => Style::default().fg(scheme.text),
            Self::HomeBlockBorder => Style::default().fg(scheme.accent),
            Self::HomeSectionHeader => Style::default().fg(scheme.text_header),
            Self::HomeOverlayBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::HomeDeleteBlock => Style::default().bg(scheme.surface).fg(scheme.danger),
            Self::HomeChangePwBlock => Style::default().bg(scheme.surface).fg(scheme.warning),
            Self::HomeNoPassBlock => Style::default().bg(scheme.surface).fg(scheme.warning),
            Self::HomePassBlock => Style::default().bg(scheme.surface).fg(scheme.accent),
            Self::HomeErrorBar => Style::default().bg(scheme.danger).fg(scheme.text),
            Self::HomePassLabel => Style::default().fg(scheme.accent),
            Self::HomeFieldLabel => Style::default().fg(scheme.text),
            Self::CursorLabel => Style::default().fg(scheme.cursor_dim),
            Self::CursorDimmed => Style::default().fg(scheme.cursor_dim),
            Self::CursorNormalText => Style::default().fg(scheme.text),
        }
    }
}
