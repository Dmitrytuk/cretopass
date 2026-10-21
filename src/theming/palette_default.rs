//! The **Default** colour scheme — one of the selectable sources of every colour
//! in the UI. Which module is active is chosen by the `pub use … as palette`
//! line in [`theming`](super); the other schemes (`palette_cyber_orchid`,
//! `palette_astro_dark`) render the same roles in different palettes.
//!
//! Every value is a TrueColor RGB colour, so the interface renders identically
//! in every terminal and never depends on the terminal's own ANSI palette.
//! ANSI slots such as `Blue`, `DarkGray` and `Gray` are re-mapped per terminal
//! and rendered dark text unreadable on black backgrounds, which is why the
//! scheme defines its own RGB values instead.
//!
//! The scheme is cool-toned: near-black indigo surfaces, soft off-white text
//! and a cyan structural accent. [`BASE_BG`] (`#141526`) is painted across the
//! entire screen before anything else, so the terminal's own background never
//! shows through; [`SURFACE`] (`#1a1b2e`) sits above it as the background of
//! every block, modal and popup.
//!
//! Nothing outside [`ElementStyle`](super::ElementStyle) maps these colours onto
//! UI elements.

use ratatui::style::Color;

// ── Surfaces ─────────────────────────────────────────────────────────────

/// Background of the whole screen, behind every panel: the Base background.
/// The darkest surface in the scheme, so panels read as raised above it.
pub const BASE_BG: Color = Color::Rgb(0x14, 0x15, 0x26);
/// Background of blocks, modals and popups. (brighter than [`BASE_BG`])
pub const SURFACE: Color = Color::Rgb(0x1a, 0x1b, 0x2e);
/// Background of a highlighted row (selected, matched or focused).
pub const SELECTION_BG: Color = Color::Rgb(0x3a, 0x40, 0x55);
/// Background of the file-browser path bar.
pub const PATH_BAR_BG: Color = Color::Rgb(0x3b, 0x42, 0x61);

// ── Text ─────────────────────────────────────────────────────────────────

/// Primary foreground text.
pub const TEXT: Color = Color::Rgb(0xd7, 0xda, 0xe0);
/// Passive, de-emphasised text such as status-bar hints.
pub const TEXT_MUTED: Color = Color::Rgb(0x9a, 0xae, 0xc9);
/// Faintest text: empty states and secondary details.
pub const TEXT_FAINT: Color = Color::Rgb(0x6b, 0x70, 0x80);
/// Section-header text.
pub const TEXT_HEADER: Color = Color::Rgb(0x58, 0x60, 0x78);
/// Dimmed cursor label.
pub const CURSOR_DIM: Color = Color::Rgb(0x7a, 0x85, 0xa0);

// ── Accents ──────────────────────────────────────────────────────────────

/// Structural accent: block borders, password markers, passphrase labels.
pub const ACCENT: Color = Color::Rgb(0x56, 0xb6, 0xc2);
/// Directory names in the file browser.
pub const DIR_NAME: Color = Color::Rgb(0x82, 0xaa, 0xff);

// ── Semantics ────────────────────────────────────────────────────────────

/// Success / affirmative (copied, add).
pub const SUCCESS: Color = Color::Rgb(0x98, 0xc3, 0x79);
/// Warning / edit.
pub const WARNING: Color = Color::Rgb(0xe5, 0xc0, 0x7b);
/// Danger / destructive.
pub const DANGER: Color = Color::Rgb(0xe0, 0x55, 0x55);
/// Search / informational (active input).
pub const INFO: Color = Color::Rgb(0x4a, 0x76, 0xb8);

// ── Status (title bar) ───────────────────────────────────────────────────

/// A blocking load (decrypt / open) is running.
pub const STATUS_LOADING: Color = Color::Rgb(0x7c, 0xfc, 0x00);
/// A save is running.
pub const STATUS_SAVING: Color = Color::Rgb(0xff, 0xd7, 0x00);
/// The latest save attempt failed.
pub const STATUS_FAILED: Color = Color::Rgb(0xff, 0x44, 0x44);

// ── Inverse ──────────────────────────────────────────────────────────────

/// Foreground drawn on a bright badge or on the inverted cursor.
pub const INVERSE_FG: Color = Color::Rgb(0x1a, 0x1b, 0x2e);
/// Background of the inverted cursor.
pub const INVERSE_BG: Color = Color::Rgb(0xd7, 0xda, 0xe0);
