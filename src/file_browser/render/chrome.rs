//! Small centred-dialog geometry helpers shared by the browser's popups.

use ratatui::layout::Rect;

/// Builds a small dialog rect centered on `area`, clamped to fit.
pub(super) fn centered_popup(area: Rect, height: u16) -> Rect {
    centered_popup_sized(area, height, 2, 3, 30)
}

/// Builds a centered dialog with a configurable width ratio and minimum width.
pub(super) fn centered_popup_sized(
    area: Rect,
    height: u16,
    numerator: u16,
    denominator: u16,
    min_width: u16,
) -> Rect {
    let height = height.min(area.height.saturating_sub(2));
    let width = (area.width.saturating_mul(numerator) / denominator)
        .max(min_width)
        .min(area.width.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
