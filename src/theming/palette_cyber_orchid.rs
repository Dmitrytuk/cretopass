//! The **Cyber Orchid** colour scheme — an alternative source of every colour in
//! the UI, selected by the `pub use … as palette` line in
//! [`theming`](super). It exposes the exact same roles as
//! [`palette_default`](super::palette_default), so `ElementStyle` maps them onto
//! UI elements unchanged.
//!
//! Every value is a TrueColor RGB colour, so the interface renders identically
//! in every terminal and never depends on the terminal's own ANSI palette.
//!
//! The scheme is built from the *Cyber Orchid* palette and is warmer than the
//! Default: deep indigo surfaces, a warm off-white text and a vivid purple-blue
//! structural accent, with magenta and raspberry for attention and danger. The
//! five source colours are Supreme Indigo `#3a3a3a`, Beyond Blue `#6a26f2`,
//! Deep White `#f5f1e6`, Awesome `#e941fb` and Raspberry `#e8114e`; the remaining
//! roles are tints of those that fit the same gamut. The recommended terminal
//! background is [`SURFACE`] (`#331a84`); the app also paints it behind every
//! block and modal.
//!
//! Nothing outside [`ElementStyle`](super::ElementStyle) maps these colours onto
//! UI elements.

use ratatui::style::Color;

// ── Surfaces ─────────────────────────────────────────────────────────────

/// Background of blocks, modals and popups. (Supreme Indigo)
pub const SURFACE: Color = Color::Rgb(0x3A, 0x3A, 0x3A);
/// Background of a highlighted row (selected, matched or focused).
pub const SELECTION_BG: Color = Color::Rgb(0x4a, 0x24, 0x98);
/// Background of the file-browser path bar.
pub const PATH_BAR_BG: Color = Color::Rgb(0x3f, 0x1e, 0x8a);

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
pub const INVERSE_FG: Color = Color::Rgb(0x33, 0x1a, 0x84);
/// Background of the inverted cursor. (= TEXT)
pub const INVERSE_BG: Color = Color::Rgb(0xf5, 0xf1, 0xe6);
