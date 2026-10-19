//! Animated busy glyph shared by the Home and Tree title bars.
//!
//! The spinner is deliberately dependency-free: the frames are the same braille
//! set `throbber-widgets-tui` ships (`BRAILLE_EIGHT`), but the crate's private
//! `i8` index saturates after ~10 s and would freeze the animation, and it pins
//! us to an older `ratatui`. A `usize` index advanced with `% FRAMES.len()`
//! stays in range by construction.

use std::time::Duration;

/// Redraw period for an active spinner (≈12.5 fps).
///
/// Shared by both screens so the Home worker-thread loop and the Tree event
/// tick animate at the same rate.
pub const SPINNER_TICK: Duration = Duration::from_millis(80);

/// Braille frames, the same set as `throbber-widgets-tui`'s `BRAILLE_EIGHT`.
const FRAMES: [&str; 8] = ["⣷", "⣯", "⣟", "⡿", "⢿", "⣻", "⣽", "⣾"];

/// A frame counter for the animated busy glyph.
#[derive(Debug, Clone, Default)]
pub struct Spinner {
    index: usize,
}

impl Spinner {
    /// Advances to the next frame, wrapping back to the first after the last.
    pub fn advance(&mut self) {
        self.index = (self.index + 1) % FRAMES.len();
    }

    /// The glyph for the current frame.
    pub fn glyph(&self) -> &'static str {
        FRAMES[self.index % FRAMES.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_glyph_is_the_first_frame() {
        assert_eq!(Spinner::default().glyph(), FRAMES[0]);
    }

    #[test]
    fn advance_walks_all_frames_and_wraps() {
        let mut spinner = Spinner::default();
        for frame in FRAMES.iter().skip(1) {
            spinner.advance();
            assert_eq!(spinner.glyph(), *frame);
        }
        spinner.advance();
        assert_eq!(spinner.glyph(), FRAMES[0]);
    }

    #[test]
    fn glyph_stays_in_range_after_many_advances() {
        let mut spinner = Spinner::default();
        for _ in 0..10_000 {
            spinner.advance();
        }
        assert!(FRAMES.contains(&spinner.glyph()));
    }
}
