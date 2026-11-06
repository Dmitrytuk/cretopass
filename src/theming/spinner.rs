//! Animated busy glyph shared by the Home and Tree title bars.
//!
//! The spinner is deliberately dependency-free: the frames come from
//! [`SpinnerStyle`]. The smoothest set is the Braille one `throbber-widgets-tui`
//! ships as `BRAILLE_EIGHT`, but Braille is outside **WGL4** — the coverage level
//! of the console fonts Windows ships — so Windows has no glyph and substitutes
//! one static placeholder for every frame. Windows therefore falls back to a
//! WGL4-safe half-block set, and a locale that does not promise UTF-8 falls back
//! to ASCII. A `usize` index advanced with `% frames().len()` stays in range by
//! construction, unlike the crate's private `i8` index that saturates after ~10 s.
//!
//! Detection ([`SpinnerStyle::detect`]) reads the process environment and the OS,
//! so the composition roots resolve it once through [`Spinner::detected`].
//! [`Spinner::default`] stays deterministic (Braille) so state builders used by
//! tests never depend on the developer's environment.

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpinnerStyle {
    /// Eight Braille frames (`U+28xx`), the same set `throbber-widgets-tui`
    /// ships as `BRAILLE_EIGHT`. The smoothest option and the deterministic
    /// [`Spinner::default`]; present in Unicode terminals, absent from the
    /// WGL4-level fonts the Windows console uses.
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

    /// Chooses a frame set from the environment and the OS.
    ///
    /// The `CRETOPASS_SPINNER` environment variable wins when it names a style;
    /// otherwise the platform decides: Windows gets [`Blocks`](Self::Blocks) —
    /// its console fonts have no Braille glyph, and a missing glyph renders one
    /// static placeholder for every frame — while every other platform gets
    /// [`Braille`](Self::Braille) when the locale is UTF-8 or unset and
    /// [`Ascii`](Self::Ascii) when the locale is explicitly non-UTF-8.
    ///
    /// Reads the environment, so call it once at a composition root (through
    /// [`Spinner::detected`]), never per frame.
    pub fn detect() -> Self {
        let choice = std::env::var(OVERRIDE_VAR).ok();
        resolve(choice.as_deref(), cfg!(windows), is_unicode_terminal())
    }
}

/// Resolves a frame set from already-read environment values.
///
/// Kept a pure function so tests can exercise every branch without mutating the
/// process-global environment, which is a data race under the default parallel
/// test runner.
fn resolve(choice: Option<&str>, windows: bool, unicode: bool) -> SpinnerStyle {
    match choice {
        Some(value) if value.eq_ignore_ascii_case("braille") => SpinnerStyle::Braille,
        Some(value) if value.eq_ignore_ascii_case("blocks") => SpinnerStyle::Blocks,
        Some(value) if value.eq_ignore_ascii_case("ascii") => SpinnerStyle::Ascii,
        _ if windows => SpinnerStyle::Blocks,
        _ if unicode => SpinnerStyle::Braille,
        _ => SpinnerStyle::Ascii,
    }
}

/// Whether this process's terminal promises UTF-8 output.
///
/// Windows console fonts are WGL4-level and the platform ignores the locale for
/// the frame choice, so the locale is not even read there.
fn is_unicode_terminal() -> bool {
    if cfg!(windows) {
        return false;
    }
    let lc_all = std::env::var("LC_ALL").ok();
    let lc_ctype = std::env::var("LC_CTYPE").ok();
    let lang = std::env::var("LANG").ok();
    is_utf8_locale(first_non_empty([
        lc_all.as_deref(),
        lc_ctype.as_deref(),
        lang.as_deref(),
    ]))
}

/// The first non-empty value among the standard locale variables, in precedence
/// order (`LC_ALL`, then `LC_CTYPE`, then `LANG`).
fn first_non_empty(values: [Option<&str>; 3]) -> Option<&str> {
    values.into_iter().flatten().find(|value| !value.is_empty())
}

/// Whether a locale string promises UTF-8 output.
///
/// An unset locale counts as UTF-8: modern terminals render UTF-8 regardless, and
/// a process launched from a GUI (common on macOS) often inherits no locale. Only
/// an explicitly non-UTF-8 locale such as `C`, `POSIX`, or `*.ISO-8859-1` gives
/// ASCII.
fn is_utf8_locale(locale: Option<&str>) -> bool {
    locale.is_none_or(|value| is_utf8_codeset(codeset(value)))
}

/// The codeset token of a POSIX locale string
/// (`language[_territory][.codeset][@modifier]`), e.g. `UTF-8` in
/// `en_US.UTF-8@euro`.
///
/// A value with no `.` is the codeset itself, so a bare `UTF-8` (the macOS
/// Terminal.app default `LC_CTYPE`) still matches. The first `.` wins and any
/// `@modifier` is dropped.
fn codeset(locale: &str) -> &str {
    let after_dot = locale.split_once('.').map_or(locale, |(_, rest)| rest);
    after_dot
        .split_once('@')
        .map_or(after_dot, |(head, _)| head)
}

/// Whether a codeset token names UTF-8 (`UTF-8` or `UTF8`, any case).
fn is_utf8_codeset(codeset: &str) -> bool {
    codeset.eq_ignore_ascii_case("UTF-8") || codeset.eq_ignore_ascii_case("UTF8")
}

/// A frame counter for the animated busy glyph.
#[derive(Debug, Clone)]
pub struct Spinner {
    index: usize,
    style: SpinnerStyle,
}

impl Default for Spinner {
    /// Builds a deterministic spinner on the Braille frame set.
    ///
    /// State builders ([`HomeState::with_config`](crate::vault::HomeState),
    /// [`TreeState::new`](crate::tree::widget::TreeState)) use this so tests never read
    /// the environment; the composition roots override it with
    /// [`Spinner::detected`].
    fn default() -> Self {
        Self::new(SpinnerStyle::Braille)
    }
}

impl Spinner {
    /// Creates a spinner that cycles through `style`'s frames.
    fn new(style: SpinnerStyle) -> Self {
        Self { index: 0, style }
    }

    /// Builds a spinner whose frame set is detected for the current terminal.
    ///
    /// Reads the environment and the OS, so call it once at a composition root.
    pub fn detected() -> Self {
        Self::new(SpinnerStyle::detect())
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
    fn default_spinner_uses_the_braille_frame_set() {
        let frames = SpinnerStyle::Braille.frames();
        assert_eq!(Spinner::default().glyph(), frames[0]);
    }

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
    fn the_first_non_empty_locale_wins_in_precedence_order() {
        assert_eq!(
            first_non_empty([Some("a"), Some("b"), Some("c")]),
            Some("a")
        );
        assert_eq!(first_non_empty([Some(""), Some("b"), Some("c")]), Some("b"));
        assert_eq!(first_non_empty([Some(""), Some(""), Some("c")]), Some("c"));
        assert_eq!(first_non_empty([None, None, Some("c")]), Some("c"));
        assert_eq!(first_non_empty([Some(""), Some(""), Some("")]), None);
        assert_eq!(first_non_empty([None, None, None]), None);
    }

    #[test]
    fn utf8_locales_are_recognised_by_their_codeset() {
        assert!(is_utf8_locale(Some("en_US.UTF-8")));
        assert!(is_utf8_locale(Some("en_us.utf-8")));
        assert!(is_utf8_locale(Some("C.utf8")));
        assert!(is_utf8_locale(Some("en_US.UTF-8@euro")));
        // A bare codeset (macOS Terminal.app's `LC_CTYPE=UTF-8`) still matches.
        assert!(is_utf8_locale(Some("UTF-8")));
        assert!(is_utf8_locale(Some("UTF8")));
        assert!(is_utf8_locale(None));
        assert!(!is_utf8_locale(Some("")));
        assert!(!is_utf8_locale(Some("C")));
        assert!(!is_utf8_locale(Some("POSIX")));
        assert!(!is_utf8_locale(Some("en_US")));
        assert!(!is_utf8_locale(Some("de_DE.ISO-8859-1")));
        // The codeset is matched as a whole token, not a substring.
        assert!(!is_utf8_locale(Some("en_US.XXUTF8XX")));
        assert!(!is_utf8_locale(Some("en_US.UTF-8x")));
    }

    #[test]
    fn codeset_takes_the_first_dot_and_drops_the_modifier() {
        assert_eq!(codeset("en_US.UTF-8"), "UTF-8");
        assert_eq!(codeset("en_US.UTF-8@euro"), "UTF-8");
        assert_eq!(codeset("UTF-8"), "UTF-8");
        assert_eq!(codeset("C"), "C");
        assert_eq!(codeset(""), "");
        assert_eq!(codeset("@euro"), "");
    }
}
