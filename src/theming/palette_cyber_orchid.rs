//! The **Cyber Orchid** colour scheme — one of the selectable sources of every
//! colour in the UI. Which module is active is chosen by the `pub use … as
//! palette` line in [`theming`](super); the other schemes (`palette_default`,
//! `palette_astro_dark`) render the same roles in different palettes. It exposes
//! the exact same roles as the other schemes, so `ElementStyle` maps them onto UI
//! elements unchanged.
//!
//! Every value is a TrueColor RGB colour, so the interface renders identically
//! in every terminal and never depends on the terminal's own ANSI palette.
//!
//! The scheme is built from the *Cyber Orchid* palette — Supreme Indigo
//! `#331a84`, Beyond Blue `#6a26f2`, Deep White `#f5f1e6`, Awesome `#e941fb` and
//! Raspberry `#e8114e` — re-tuned to a warmer look: warm off-white text, a vivid
//! purple-blue structural accent, and magenta/raspberry for attention and danger.
//! The surface family (`BASE_BG`, `SURFACE`, `SELECTION_BG`, `PATH_BAR_BG`,
//! `INVERSE_FG`) is neutral grey rather than the palette's own indigo, so panels
//! and the cursor do not assume an indigo terminal. [`BASE_BG`] (`#303030`) is
//! painted across the entire screen before anything else, so the terminal's own
//! background never shows through; [`SURFACE`] (`#3a3a3a`) sits above it as the
//! background of every block, modal and popup.
//!
//! Nothing outside [`ElementStyle`](super::ElementStyle) maps these colours onto
//! UI elements.

use ratatui::style::Color;

// ── Surfaces ─────────────────────────────────────────────────────────────

/// Background of the whole screen, behind every panel: the Base background.
/// The darkest surface in the scheme, so panels read as raised above it.
/// (neutral grey)
pub const BASE_BG: Color = Color::Rgb(0x30, 0x30, 0x30);
/// Background of blocks, modals and popups. (neutral grey, brighter than
/// [`BASE_BG`])
pub const SURFACE: Color = Color::Rgb(0x3A, 0x3A, 0x3A);
/// Background of a highlighted row (selected, matched or focused). (lighter grey)
pub const SELECTION_BG: Color = Color::Rgb(0x4A, 0x4A, 0x4A);
/// Background of the file-browser path bar. (grey, between SURFACE and SELECTION_BG)
pub const PATH_BAR_BG: Color = Color::Rgb(0x42, 0x42, 0x42);

// ── Text ─────────────────────────────────────────────────────────────────

/// Primary foreground text. (Deep White)
pub const TEXT: Color = Color::Rgb(0xf5, 0xf1, 0xe6);
/// Passive, de-emphasised text such as status-bar hints.
pub const TEXT_MUTED: Color = Color::Rgb(0x9a, 0x8b, 0xc4);
/// Faintest text: empty states and secondary details.
pub const TEXT_FAINT: Color = Color::Rgb(0x6b, 0x5c, 0x94);
/// Section-header text.
pub const TEXT_HEADER: Color = Color::Rgb(0x5a, 0x4a, 0x80);
/// Dimmed cursor label.
pub const CURSOR_DIM: Color = Color::Rgb(0x7a, 0x6b, 0xa0);

// ── Accents ──────────────────────────────────────────────────────────────

/// Structural accent: block borders, password markers, passphrase labels.
/// (Beyond Blue)
pub const ACCENT: Color = Color::Rgb(0x6a, 0x26, 0xf2);
/// Directory names in the file browser.
pub const DIR_NAME: Color = Color::Rgb(0x8a, 0x5a, 0xf5);

// ── Semantics ────────────────────────────────────────────────────────────

/// Success / affirmative (copied, add).
pub const SUCCESS: Color = Color::Rgb(0x5e, 0xc4, 0x9e);
/// Warning / edit.
pub const WARNING: Color = Color::Rgb(0xd4, 0xa8, 0x4b);
/// Danger / destructive. (Raspberry)
pub const DANGER: Color = Color::Rgb(0xe8, 0x11, 0x4e);
/// Search / informational (active input). (Awesome)
pub const INFO: Color = Color::Rgb(0xe9, 0x41, 0xfb);

// ── Status (title bar) ───────────────────────────────────────────────────

/// A blocking load (decrypt / open) is running.
pub const STATUS_LOADING: Color = Color::Rgb(0x5c, 0xe0, 0xa8);
/// A save is running.
pub const STATUS_SAVING: Color = Color::Rgb(0xf0, 0xc8, 0x4a);
/// The latest save attempt failed. (Raspberry)
pub const STATUS_FAILED: Color = Color::Rgb(0xe8, 0x11, 0x4e);

// ── Inverse ──────────────────────────────────────────────────────────────

/// Foreground drawn on a bright badge or on the inverted cursor. (= SURFACE)
pub const INVERSE_FG: Color = Color::Rgb(0x3A, 0x3A, 0x3A);
/// Background of the inverted cursor. (= TEXT)
pub const INVERSE_BG: Color = Color::Rgb(0xf5, 0xf1, 0xe6);
