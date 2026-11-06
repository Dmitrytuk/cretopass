pub mod alert;
pub mod background;
pub mod icon;
pub mod key;
pub mod palette_default_dark;
pub mod palette_default_light;
pub mod spinner;
pub mod status;
pub mod status_bar;
pub mod style;
pub mod texts;

use std::sync::RwLock;

use ratatui::style::Color;

pub use alert::{AlertKind, render_alert};
pub use background::paint_background;
pub use icon::Icon;
pub use key::{Action, is_clipboard_paste, wheel_nav_key};
pub use spinner::{SPINNER_TICK, Spinner};
pub use status::Status;
pub use status_bar::render_status_bar;
pub use style::ElementStyle;

// ── Active colour scheme ───────────────────────────────────────────────────
// The active scheme is held in a process-wide cell so every `ElementStyle`
// resolves against it without threading a palette through every render call.
// It changes only when the user toggles it in the Configuration modal.

/// Canonical config name of the **Default Dark** scheme.
pub const SCHEME_DARK: &str = "dark";
/// Canonical config name of the **Default Light** scheme.
pub const SCHEME_LIGHT: &str = "light";

/// One complete colour scheme: every colour role the UI uses, as a TrueColor
/// RGB value.
///
/// A scheme is a plain data value, so switching schemes is a structural
/// assignment rather than a rebuild of the render tree. [`ElementStyle`] is the
/// only thing that maps these roles onto UI elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Background painted across the whole screen, behind every panel.
    pub base_bg: Color,
    /// Background of blocks, modals and popups.
    pub surface: Color,
    /// Background of a highlighted row and of the neutral status-bar badges.
    pub selection_bg: Color,
    /// Background of the file-browser path bar.
    pub path_bar_bg: Color,
    /// Primary foreground text.
    pub text: Color,
    /// Passive, de-emphasised text such as status-bar hints.
    pub text_muted: Color,
    /// Faintest text: empty states and secondary details.
    pub text_faint: Color,
    /// Section-header text.
    pub text_header: Color,
    /// Dimmed cursor label.
    pub cursor_dim: Color,
    /// Structural accent: block borders, password markers, passphrase labels.
    pub accent: Color,
    /// Directory names in the file browser.
    pub dir_name: Color,
    /// Success / affirmative (copied, add).
    pub success: Color,
    /// Warning / edit.
    pub warning: Color,
    /// Danger / destructive.
    pub danger: Color,
    /// Search / informational (active input).
    pub info: Color,
    /// A blocking load (decrypt / open) is running.
    pub status_loading: Color,
    /// A save is running.
    pub status_saving: Color,
    /// The latest save attempt failed.
    pub status_failed: Color,
    /// Foreground drawn on a bright badge or on the inverted cursor.
    pub inverse_fg: Color,
    /// Background of the inverted cursor.
    pub inverse_bg: Color,
}

impl Palette {
    /// Returns the scheme registered under `name`, falling back to the
    /// **Default Dark** scheme when the name is unknown.
    pub fn from_name(name: &str) -> Self {
        match name {
            SCHEME_LIGHT => palette_default_light::VALUES,
            _ => palette_default_dark::VALUES,
        }
    }
}

/// The active colour scheme, seeded with **Default Dark**.
static ACTIVE: RwLock<Palette> = RwLock::new(palette_default_dark::VALUES);

/// Returns a copy of the active colour scheme.
///
/// The value is small and `Copy`, so callers hold their own snapshot for the
/// duration of a render instead of borrowing the lock.
pub fn palette() -> Palette {
    match ACTIVE.read() {
        Ok(guard) => *guard,
        // A poisoned lock only means a previous writer panicked; the value it
        // holds is still a valid scheme, so recover it rather than propagate.
        Err(poisoned) => *poisoned.into_inner(),
    }
}

/// Makes `scheme` the active colour scheme for all subsequent rendering.
pub fn set_palette(scheme: Palette) {
    match ACTIVE.write() {
        Ok(mut guard) => *guard = scheme,
        Err(poisoned) => *poisoned.into_inner() = scheme,
    }
}

/// Returns the canonical name of the scheme to switch to when the user toggles
/// the colour scheme from `name`.
///
/// Anything other than [`SCHEME_LIGHT`] — including an unknown name, which
/// [`Palette::from_name`] renders as **Default Dark** — toggles to
/// [`SCHEME_LIGHT`].
pub fn toggled_scheme(name: &str) -> &'static str {
    if name == SCHEME_LIGHT {
        SCHEME_DARK
    } else {
        SCHEME_LIGHT
    }
}

/// Returns the human-readable label for the scheme registered under `name`.
pub fn scheme_label(name: &str) -> &'static str {
    if name == SCHEME_LIGHT {
        texts::PALETTE_LIGHT
    } else {
        texts::PALETTE_DARK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_name_resolves_the_known_schemes() {
        assert_eq!(
            Palette::from_name(SCHEME_DARK),
            palette_default_dark::VALUES
        );
        assert_eq!(
            Palette::from_name(SCHEME_LIGHT),
            palette_default_light::VALUES
        );
    }

    #[test]
    fn from_name_falls_back_to_dark() {
        assert_eq!(
            Palette::from_name("no-such-scheme"),
            palette_default_dark::VALUES
        );
    }

    #[test]
    fn set_palette_switches_the_active_scheme() {
        set_palette(palette_default_light::VALUES);
        assert_eq!(palette(), palette_default_light::VALUES);

        // Restore the default so a later test never renders a stale scheme.
        set_palette(palette_default_dark::VALUES);
        assert_eq!(palette(), palette_default_dark::VALUES);
    }

    #[test]
    fn toggled_scheme_flips_between_the_two() {
        assert_eq!(toggled_scheme(SCHEME_DARK), SCHEME_LIGHT);
        assert_eq!(toggled_scheme(SCHEME_LIGHT), SCHEME_DARK);
        assert_eq!(toggled_scheme("unknown"), SCHEME_LIGHT);
    }
}
