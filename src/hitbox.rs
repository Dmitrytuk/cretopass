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

/// The clickable rows of one panel together with the panel's own rectangle.
///
/// A modal keeps one of these: each frame it [`clear`](ClickSurface::clear)s the
/// surface, [`set_panel`](ClickSurface::set_panel)s the rect it drew, and
/// [`record`](ClickSurface::record)s one row per clickable line. A click is then
/// either a [`target_at`](ClickSurface::target_at) row or, via [`clicked_outside`],
/// a miss the dispatcher turns into the modal's `Esc`. Bundling the panel with
/// the rows is what lets every modal share the dismissal rule instead of
/// re-deriving `is_left && panel && !point_in`.
#[derive(Debug)]
pub(crate) struct ClickSurface<T> {
    panel: Option<Rect>,
    hits: HitMap<T>,
}

impl<T> Default for ClickSurface<T> {
    fn default() -> Self {
        Self {
            panel: None,
            hits: HitMap::default(),
        }
    }
}

impl<T> ClickSurface<T> {
    /// Forgets every region **and the panel**, ready to record the next frame.
    ///
    /// Dropping the panel matters: a renderer that forgets to [`set_panel`] this
    /// frame leaves its modal undismissable, rather than dismissable against a
    /// stale rect.
    ///
    /// [`set_panel`]: ClickSurface::set_panel
    pub(crate) fn clear(&mut self) {
        self.panel = None;
        self.hits.clear();
    }

    /// Remembers the panel rectangle the renderer drew this frame.
    pub(crate) fn set_panel(&mut self, rect: Rect) {
        self.panel = Some(rect);
    }

    /// Records a left click anywhere inside `rect` as `target`.
    pub(crate) fn record(&mut self, rect: Rect, target: T) {
        self.record_with(rect, MouseButton::Left, target);
    }

    /// Records a click with an explicit `button` inside `rect` as `target`.
    ///
    /// The common case is a left-clickable row ([`record`](ClickSurface::record));
    /// this is for the rare surface that answers a second button too, like the
    /// Detail modal's browsed field rows, where the right button copies a value.
    pub(crate) fn record_with(&mut self, rect: Rect, button: MouseButton, target: T) {
        self.hits.push(rect, button, target);
    }

    /// The panel rectangle once it has been drawn, or `None` before the first
    /// frame — so a modal that was never rendered survives a stray click.
    pub(crate) fn panel(&self) -> Option<Rect> {
        self.panel
    }
}

impl<T: Copy> ClickSurface<T> {
    /// The target of a click at `(x, y)`, if it hit a recorded row.
    pub(crate) fn target_at(&self, button: MouseButton, x: u16, y: u16) -> Option<T> {
        self.hits.hit(x, y, button)
    }
}

/// Whether `button` at `(x, y)` is a click outside `panel`, once it has been
/// drawn.
///
/// This is the modal's "outside click = Esc" rule, defined once. It is a free
/// function rather than only a [`ClickSurface`] method because a screen's modal
/// dispatcher is a sum type: it can hand over only the active variant's *panel*
/// (through `panel_rect`), not that variant's whole surface.
pub(crate) fn clicked_outside(panel: Option<Rect>, button: MouseButton, x: u16, y: u16) -> bool {
    is_left(button) && panel.is_some_and(|rect| !point_in(rect, x, y))
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
    fn click_surface_resolves_rows_and_reports_outside_clicks() {
        let mut surface: ClickSurface<Target> = ClickSurface::default();
        assert_eq!(surface.panel(), None);
        assert!(
            !clicked_outside(surface.panel(), MouseButton::Left, 0, 0),
            "an unrendered panel is never 'outside'"
        );

        surface.set_panel(Rect::new(2, 2, 6, 3));
        surface.record(Rect::new(2, 3, 6, 1), Target::A);

        assert_eq!(surface.panel(), Some(Rect::new(2, 2, 6, 3)));
        assert_eq!(surface.target_at(MouseButton::Left, 3, 3), Some(Target::A));
        assert_eq!(surface.target_at(MouseButton::Left, 3, 2), None);
        assert_eq!(
            surface.target_at(MouseButton::Right, 3, 3),
            None,
            "record is left-only; the modals' dropped guards depend on it"
        );
        assert!(!clicked_outside(surface.panel(), MouseButton::Left, 3, 3));
        assert!(clicked_outside(surface.panel(), MouseButton::Left, 0, 0));
        assert!(
            !clicked_outside(surface.panel(), MouseButton::Right, 0, 0),
            "only the left button dismisses"
        );
    }

    #[test]
    fn clearing_a_click_surface_forgets_panel_and_rows() {
        let mut surface: ClickSurface<Target> = ClickSurface::default();
        surface.set_panel(Rect::new(2, 2, 6, 3));
        surface.record(Rect::new(2, 3, 6, 1), Target::A);

        surface.clear();

        assert_eq!(surface.panel(), None);
        assert_eq!(surface.target_at(MouseButton::Left, 3, 3), None);
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
