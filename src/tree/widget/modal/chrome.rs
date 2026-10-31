//! Small geometry helpers shared by the Tree's modals.

use ratatui::layout::Rect;

/// A centred overlay tall enough for `base_content` lines plus one row per extra
/// field, clamped to the terminal.
pub(crate) fn make_overlay_area(area: Rect, extra_fields_len: usize, base_content: u16) -> Rect {
    let n = extra_fields_len as u16;
    let content_height = base_content.saturating_add(n);
    let overlay_height = (content_height + 2).min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 5,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width * 3 / 5,
        height: overlay_height,
    }
}

/// A fixed-size overlay of `width` × `height` centred in `area`.
///
/// Both dimensions are clamped to `area` and centred with saturating arithmetic,
/// so a terminal smaller than the overlay can never underflow or overflow.
pub(crate) fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_rect_stays_inside_a_terminal_smaller_than_the_overlay() {
        let area = Rect::new(0, 0, 2, 2);
        let rect = centered_rect(area, 40, 8);
        assert_eq!(rect.width, 2);
        assert_eq!(rect.height, 2);
        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 0);
        assert!(rect.right() <= area.right());
        assert!(rect.bottom() <= area.bottom());
    }

    #[test]
    fn centered_rect_centres_when_it_fits() {
        let area = Rect::new(0, 0, 20, 10);
        assert_eq!(centered_rect(area, 10, 4), Rect::new(5, 3, 10, 4));
    }
}
