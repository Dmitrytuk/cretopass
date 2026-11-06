//! Animated busy glyph shared by the Home and Tree title bars.
//!
//! The spinner is deliberately dependency-free: the frames come from
//! [`SpinnerStyle`], chosen at runtime by [`SpinnerStyle::detect`]. The smoothest
//! set is the Braille one `throbber-widgets-tui` ships as `BRAILLE_EIGHT`, but
//! Braille is outside **WGL4** — the coverage level of the console fonts Windows
//! ships — so Windows has no glyph and substitutes one static placeholder for
//! every frame. Windows therefore falls back to a WGL4-safe half-block set, and
//! a locale that does not promise UTF-8 falls back to ASCII. A `usize` index
//! advanced with `% frames().len()` stays in range by construction, unlike the
//! crate's private `i8` index that saturates after ~10 s.

use std::time::Duration;

/// Redraw period for an active spinner (≈12.5 fps).
///
/// Shared by both screens so the Home worker-thread loop and the Tree event
/// tick animate at the same rate.
pub const SPINNER_TICK: Duration = Duration::from_millis(80);

/// Environment variable that forces a frame set.
///
/// Accepts `auto`, `braille`, `blocks`, or `ascii` (case-insensitive); any other
/// value — including an empty one — is treated as `auto` and follows the
/// platform policy. Matches the `CRETOPASS_*` prefix used elsewhere.
const OVERRIDE_VAR: &str = "CRETOPASS_SPINNER";

/// The frame set a terminal can render.
///
/// A set is never mixed: each style is a self-contained animation, so a terminal
/// that lacks the glyphs of one set (the Windows console, for Braille) still
/// animates with another.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpinnerStyle {
    /// Eight Braille frames (`U+28xx`), the same set `throbber-widgets-tui`
    /// ships as `BRAILLE_EIGHT`. The default and the smoothest option; present
    /// in Unicode terminals, absent from the WGL4-level fonts the Windows
    /// console uses.
    #[default]
    Braille,
    /// A rotating half-block set. Every glyph is in **WGL4** (`U+2580` `▀`,
    /// `U+2590` `▐`, `U+2584` `▄`, `U+258C` `▌`), so it renders in the Windows
    /// console fonts where Braille does not. Falling back to it is what keeps
    /// the "working vs. frozen" signal on Windows.
    Blocks,
    /// Four ASCII frames, for a locale that does not promise UTF-8.
    Ascii,
}

impl SpinnerStyle {
    /// The frames this style cycles through, in order.
    fn frames(self) -> &'static [&'static str] {
        match self {
            Self::Braille => &["⣷", "⣯", "⣟", "⡿", "⢿", "⣻", "⣽", "⣾"],
            Self::Blocks => &["▀", "▐", "▄", "▌"],
            Self::Ascii => &["|", "/", "-", "\\"],
        }
    }

    /// Chooses a frame set from the environment.
    ///
    /// The `CRETOPASS_SPINNER` environment variable wins when it names a style;
    /// otherwise the platform decides: Windows gets [`Blocks`](Self::Blocks) —
    /// its console fonts have no Braille glyph, and a missing glyph renders one
    /// static placeholder for every frame — while every other platform gets
    /// [`Braille`](Self::Braille) when the locale is UTF-8 and
    /// [`Ascii`](Self::Ascii) otherwise.
    pub fn detect() -> Self {
        let choice = std::env::var(OVERRIDE_VAR).ok();
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()));
        resolve(
            choice.as_deref(),
            cfg!(windows),
            is_utf8_locale(locale.as_deref()),
        )
    }
}

/// Resolves a frame set from already-read environment values.
///
/// Kept a pure function so tests can exercise every branch without mutating the
/// process-global environment, which is undefined behaviour under the default
/// parallel test runner.
fn resolve(choice: Option<&str>, windows: bool, unicode: bool) -> SpinnerStyle {
    match choice.map(str::to_ascii_lowercase).as_deref() {
        Some("braille") => SpinnerStyle::Braille,
        Some("blocks") => SpinnerStyle::Blocks,
        Some("ascii") => SpinnerStyle::Ascii,
        _ if windows => SpinnerStyle::Blocks,
        _ if unicode => SpinnerStyle::Braille,
        _ => SpinnerStyle::Ascii,
    }
}

/// Whether a locale string (e.g. `en_US.UTF-8`) promises UTF-8 output.
fn is_utf8_locale(locale: Option<&str>) -> bool {
    locale.is_some_and(|value| {
        let upper = value.to_ascii_uppercase();
        upper.contains("UTF-8") || upper.contains("UTF8")
    })
}

/// A frame counter for the animated busy glyph.
#[derive(Debug, Clone)]
pub struct Spinner {
    index: usize,
    style: SpinnerStyle,
}

impl Default for Spinner {
    /// Builds a spinner whose frame set is detected for the current terminal.
    fn default() -> Self {
        Self::new(SpinnerStyle::detect())
    }
}

impl Spinner {
    /// Creates a spinner that cycles through `style`'s frames.
    pub fn new(style: SpinnerStyle) -> Self {
        Self { index: 0, style }
    }

    /// Advances to the next frame, wrapping back to the first after the last.
    pub fn advance(&mut self) {
        self.index = (self.index + 1) % self.style.frames().len();
    }

    /// The glyph for the current frame.
    pub fn glyph(&self) -> &'static str {
        let frames = self.style.frames();
        frames[self.index % frames.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STYLES: [SpinnerStyle; 3] = [
        SpinnerStyle::Braille,
        SpinnerStyle::Blocks,
        SpinnerStyle::Ascii,
    ];

    #[test]
    fn every_style_advances_through_its_frames_and_wraps() {
        for style in STYLES {
            let frames = style.frames();
            let mut spinner = Spinner::new(style);

            assert_eq!(spinner.glyph(), frames[0]);
            for frame in frames.iter().skip(1) {
                spinner.advance();
                assert_eq!(spinner.glyph(), *frame);
            }
            spinner.advance();
            assert_eq!(spinner.glyph(), frames[0]);
        }
    }

    #[test]
    fn every_style_stays_in_range_after_many_advances() {
        for style in STYLES {
            let mut spinner = Spinner::new(style);
            for _ in 0..10_000 {
                spinner.advance();
            }
            assert!(style.frames().contains(&spinner.glyph()));
        }
    }

    #[test]
    fn an_explicit_choice_overrides_the_platform() {
        assert_eq!(resolve(Some("braille"), true, false), SpinnerStyle::Braille);
        assert_eq!(resolve(Some("Blocks"), true, false), SpinnerStyle::Blocks);
        assert_eq!(resolve(Some("ASCII"), false, true), SpinnerStyle::Ascii);
    }

    #[test]
    fn an_absent_or_unknown_choice_follows_the_platform() {
        for choice in [None, Some(""), Some("auto"), Some("nonsense")] {
            assert_eq!(resolve(choice, true, true), SpinnerStyle::Blocks);
            assert_eq!(resolve(choice, false, true), SpinnerStyle::Braille);
            assert_eq!(resolve(choice, false, false), SpinnerStyle::Ascii);
        }
    }

    #[test]
    fn utf8_locales_are_recognised_by_their_common_spellings() {
        assert!(is_utf8_locale(Some("en_US.UTF-8")));
        assert!(is_utf8_locale(Some("C.utf8")));
        assert!(!is_utf8_locale(Some("C")));
        assert!(!is_utf8_locale(None));
    }
}
