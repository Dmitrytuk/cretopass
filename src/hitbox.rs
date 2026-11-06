//! Rect-to-target hit testing for mouse clicks.
//!
//! A renderer knows the [`Rect`] of every clickable region it draws, so it
//! records one [`HitBox`] per region into a [`HitMap`] every frame. The event
//! loop then asks the map what a click at a screen cell means, and the screen
//! turns that target into an existing `Action`. Keeping the geometry in the
//! renderer is what stops a click and its key from drifting apart: both end up
//! at the same action.
//!
//! The map is generic over the screen's own target, so this module depends only
//! on `ratatui` and `crossterm` — a strict leaf like `generator` and `clipboard`.

use crossterm::event::MouseButton;
use ratatui::layout::Rect;

/// Whether `button` is the primary (left) button.
///
/// Every clickable control in the app answers the left button; the right button
/// is meaningful only on the Detail modal's field rows.
pub(crate) fn is_left(button: MouseButton) -> bool {
    matches!(button, MouseButton::Left)
}

/// Whether the cell `(x, y)` falls inside `rect`.
///
/// `ratatui` 0.28 has no `Rect::contains`, so this is the one containment
/// definition the app hit-tests with: [`HitBox::contains`] delegates to it, and
/// a modal uses it to tell a click inside its panel from one outside it. Width
/// and height are added with saturating arithmetic, so an overflowing rect can
/// never wrap and report a cell outside it.
pub(crate) fn point_in(rect: Rect, x: u16, y: u16) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// One clickable rectangle and what a click there means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HitBox<T> {
    rect: Rect,
    button: MouseButton,
    target: T,
}

impl<T> HitBox<T> {
    /// Whether `(x, y)` falls inside this box.
    fn contains(&self, x: u16, y: u16) -> bool {
        point_in(self.rect, x, y)
    }
}

/// The clickable regions of one frame, in the order they were recorded.
///
/// A later box is "on top": [`HitMap::hit`] returns the topmost match, so a
/// modal's rows outrank the screen rows drawn underneath them.
#[derive(Debug)]
pub(crate) struct HitMap<T> {
    boxes: Vec<HitBox<T>>,
}

impl<T> Default for HitMap<T> {
    fn default() -> Self {
        Self { boxes: Vec::new() }
    }
}

impl<T> HitMap<T> {
    /// Forgets every box, ready to record the next frame.
    pub(crate) fn clear(&mut self) {
        self.boxes.clear();
    }

    /// Records `target` for a click with `button` anywhere inside `rect`.
    pub(crate) fn push(&mut self, rect: Rect, button: MouseButton, target: T) {
        self.boxes.push(HitBox {
            rect,
            button,
            target,
        });
    }
}

impl<T: Copy> HitMap<T> {
    /// The target of the topmost box that contains `(x, y)` and matches
    /// `button`, if any.
    pub(crate) fn hit(&self, x: u16, y: u16, button: MouseButton) -> Option<T> {
        self.boxes
            .iter()
            .rev()
            .find(|b| b.button == button && b.contains(x, y))
            .map(|b| b.target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Target {
        A,
        B,
    }

    fn map() -> HitMap<Target> {
        let mut hits = HitMap::default();
        hits.push(Rect::new(0, 0, 10, 3), MouseButton::Left, Target::A);
        hits.push(Rect::new(2, 1, 4, 1), MouseButton::Left, Target::B);
        hits
    }

    #[test]
    fn a_click_inside_a_box_returns_its_target() {
        assert_eq!(map().hit(0, 0, MouseButton::Left), Some(Target::A));
        assert_eq!(map().hit(9, 2, MouseButton::Left), Some(Target::A));
    }

    #[test]
    fn a_click_outside_every_box_returns_none() {
        assert_eq!(map().hit(10, 0, MouseButton::Left), None);
        assert_eq!(map().hit(0, 3, MouseButton::Left), None);
    }

    #[test]
    fn the_topmost_box_wins() {
        assert_eq!(map().hit(3, 1, MouseButton::Left), Some(Target::B));
    }

    #[test]
    fn a_box_only_matches_its_own_button() {
        assert_eq!(map().hit(0, 0, MouseButton::Right), None);
    }

    #[test]
    fn clearing_forgets_every_box() {
        let mut hits = map();
        hits.clear();
        assert_eq!(hits.hit(0, 0, MouseButton::Left), None);
    }

    #[test]
    fn is_left_only_accepts_the_primary_button() {
        assert!(is_left(MouseButton::Left));
        assert!(!is_left(MouseButton::Right));
    }

    #[test]
    fn point_in_matches_the_half_open_rectangle() {
        let rect = Rect::new(2, 1, 4, 3);
        assert!(point_in(rect, 2, 1), "the top-left corner is inside");
        assert!(point_in(rect, 5, 3), "the last cell is inside");
        assert!(!point_in(rect, 1, 1), "left of the rect is outside");
        assert!(!point_in(rect, 6, 1), "right of the rect is outside");
        assert!(!point_in(rect, 2, 0), "above the rect is outside");
        assert!(!point_in(rect, 2, 4), "below the rect is outside");
    }

    #[test]
    fn point_in_saturates_instead_of_wrapping() {
        // A rect whose right edge would overflow `u16` must not wrap and claim
        // cells on the far left of the screen.
        let rect = Rect::new(u16::MAX - 2, 0, 10, 1);
        assert!(
            !point_in(rect, 0, 0),
            "the rect must not wrap to the left edge"
        );
        assert!(point_in(rect, u16::MAX - 2, 0));
        assert!(point_in(rect, u16::MAX - 1, 0));
        assert!(
            !point_in(rect, u16::MAX, 0),
            "the saturated right edge stays exclusive"
        );
    }
}
