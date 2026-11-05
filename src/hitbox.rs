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
        let (left, top, width, height) =
            (self.rect.x, self.rect.y, self.rect.width, self.rect.height);
        x >= left && x < left.saturating_add(width) && y >= top && y < top.saturating_add(height)
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
}
