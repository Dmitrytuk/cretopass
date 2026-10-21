//! The **Default Dark** colour scheme — the single source of every colour in the
//! UI, exposed as `palette` by the `pub use … as palette` line in
//! [`theming`](super). It defines every colour role, so `ElementStyle` maps them
//! onto UI elements directly.
//!
//! Every value is a TrueColor RGB colour, so the interface renders identically
//! in every terminal and never depends on the terminal's own ANSI palette.
//!
//! The scheme started from the *VS Code Astro Dark* theme, but its surfaces have
//! been re-tuned to mid greys: a light canvas (`#2c2c2c`) with darker panels
//! (`#202020`) recessed into it. The rest is derived from that theme — a brighter
//! grey-blue foreground (`#b0b0be`), a periwinkle structural accent (`#9ea1c8`),
//! sky-blue directories (`#5cc9f5`), mint success (`#23ebc0`), warm-gold warning
//! (`#ffd493`) and muted-violet information (`#6c5e8a`). [`BASE_BG`] (`#2c2c2c`)
//! is painted across the entire screen before anything else, so the terminal's
//! own background never shows through; [`SURFACE`] (`#202020`) sits *below* it,
//! so every block, modal and popup reads as recessed into the canvas.
//!
//! Nothing outside [`ElementStyle`](super::ElementStyle) maps these colours onto
//! UI elements.

use ratatui::style::Color;

// ── Surfaces ─────────────────────────────────────────────────────────────

/// Background of the whole screen, behind every panel: the Base background.
/// The lighter of the two main surfaces here, so blocks, modals and popups read
/// as recessed into it.
pub const BASE_BG: Color = Color::Rgb(0x2C, 0x2C, 0x2C);
/// Background of blocks, modals and popups. (a darker grey than [`BASE_BG`], so
/// the panels it fills read as recessed into the canvas)
pub const SURFACE: Color = Color::Rgb(0x20, 0x20, 0x20);
/// Background of a highlighted row (selected, matched or focused) and of the
/// neutral status-bar badges (`NORMAL` / `DETAIL` / `HELP`).
pub const SELECTION_BG: Color = Color::Rgb(0x3A, 0x3D, 0x4E);
/// Background of the file-browser path bar. (darker than both [`SURFACE`] and
/// [`SELECTION_BG`])
pub const PATH_BAR_BG: Color = Color::Rgb(0x13, 0x15, 0x1A);

// ── Text ─────────────────────────────────────────────────────────────────

/// Primary foreground text: brighter than the editor's own foreground so menu
/// and tree rows stay legible over the [`BASE_BG`] canvas.
pub const TEXT: Color = Color::Rgb(0xB0, 0xB0, 0xBE);
/// Passive, de-emphasised text such as status-bar hints.
pub const TEXT_MUTED: Color = Color::Rgb(0x68, 0x69, 0x74);
/// Faintest text: empty states and secondary details.
pub const TEXT_FAINT: Color = Color::Rgb(0x43, 0x44, 0x4D);
/// Section-header text.
pub const TEXT_HEADER: Color = Color::Rgb(0x68, 0x69, 0x74);
/// Dimmed cursor label.
pub const CURSOR_DIM: Color = Color::Rgb(0x81, 0x7C, 0x9C);

// ── Accents ──────────────────────────────────────────────────────────────

/// Structural accent: block borders, password markers, passphrase labels.
/// (slate-lavender; more saturated than the surrounding text so borders stay legible)
pub const ACCENT: Color = Color::Rgb(0x9E, 0xA1, 0xC8);
/// Directory names in the file browser. (sky blue)
pub const DIR_NAME: Color = Color::Rgb(0x5C, 0xC9, 0xF5);

// ── Semantics ────────────────────────────────────────────────────────────

/// Success / affirmative (copied, add). (mint)
pub const SUCCESS: Color = Color::Rgb(0x23, 0xEB, 0xC0);
/// Warning / edit. (warm gold)
pub const WARNING: Color = Color::Rgb(0xFF, 0xD4, 0x93);
/// Danger / destructive. (soft red)
pub const DANGER: Color = Color::Rgb(0xE0, 0x6C, 0x75);
/// Search / informational (active input). (muted violet)
pub const INFO: Color = Color::Rgb(0x6C, 0x5E, 0x8A);

// ── Status (title bar) ───────────────────────────────────────────────────

/// A blocking load (decrypt / open) is running. (warm gold)
pub const STATUS_LOADING: Color = Color::Rgb(0xFF, 0xD4, 0x93);
/// A save is running. (salmon)
pub const STATUS_SAVING: Color = Color::Rgb(0xFF, 0x9E, 0x93);
/// The latest save attempt failed. (soft red)
pub const STATUS_FAILED: Color = Color::Rgb(0xE0, 0x6C, 0x75);

// ── Inverse ──────────────────────────────────────────────────────────────

/// Foreground drawn on a bright badge or on the inverted cursor. (near-black ink,
/// deliberately not `SURFACE` now that the panels are mid grey)
pub const INVERSE_FG: Color = Color::Rgb(0x0F, 0x10, 0x14);
/// Background of the inverted cursor. (= TEXT)
pub const INVERSE_BG: Color = Color::Rgb(0xB0, 0xB0, 0xBE);
