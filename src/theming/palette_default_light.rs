//! The **Default Light** colour scheme — the light counterpart of
//! [`palette_default_dark`](super::palette_default_dark). It mirrors that file
//! exactly, one TrueColor RGB constant per colour role, so `ElementStyle` can
//! drive the same UI from either scheme by swapping which one is re-exported as
//! `palette` in [`theming`](super).
//!
//! Every value is a TrueColor RGB colour, so the interface renders identically
//! in every terminal and never depends on the terminal's own ANSI palette.
//!
//! Where Default Dark is a cool mid-grey scheme, Default Light is a warm one: a
//! pale off-white canvas ([`BASE_BG`] `#f4f1ea`) with cream panels ([`SURFACE`]
//! `#e7e2d8`) recessed into it, and a dark ink ([`TEXT`] `#3a3a46`) for
//! legibility on the light surfaces. The structural accent stays a periwinkle
//! ([`ACCENT`] `#6c6fa8`), directories stay sky blue ([`DIR_NAME`] `#2e7db0`),
//! success stays mint ([`SUCCESS`] `#12876a`), warning stays warm gold
//! ([`WARNING`] `#a9761b`) and information stays muted violet ([`INFO`]
//! `#c4b8dc`). As in the dark scheme [`BASE_BG`] is painted across the entire
//! screen before anything else, so the terminal's own background never shows
//! through, and [`SURFACE`] is a *different* colour from the canvas so every
//! block, modal and popup reads as a distinct panel.
//!
//! Unlike Default Dark, the semantic colours here are dark enough to read as
//! foreground text on the light surfaces, so the ink drawn on them is the light
//! [`INVERSE_FG`] rather than a near-black ink.
//!
//! Nothing outside [`ElementStyle`](super::ElementStyle) maps these colours onto
//! UI elements.

use ratatui::style::Color;

// ── Surfaces ─────────────────────────────────────────────────────────────

/// Background of the whole screen, behind every panel: the Base background.
/// The lighter of the two main surfaces here, so blocks, modals and popups read
/// as recessed into it.
pub const BASE_BG: Color = Color::Rgb(0xF4, 0xF1, 0xEA);
/// Background of blocks, modals and popups. (a darker cream than [`BASE_BG`], so
/// the panels it fills read as recessed into the canvas)
pub const SURFACE: Color = Color::Rgb(0xE7, 0xE2, 0xD8);
/// Background of a highlighted row (selected, matched or focused) and of the
/// neutral status-bar badges (`NORMAL` / `DETAIL` / `HELP`).
pub const SELECTION_BG: Color = Color::Rgb(0xD6, 0xDA, 0xE4);
/// Background of the file-browser path bar. (darker than both [`SURFACE`] and
/// [`SELECTION_BG`])
pub const PATH_BAR_BG: Color = Color::Rgb(0xD3, 0xCD, 0xC3);

// ── Text ─────────────────────────────────────────────────────────────────

/// Primary foreground text: darker than the editor's own foreground so menu
/// and tree rows stay legible over the [`BASE_BG`] canvas.
pub const TEXT: Color = Color::Rgb(0x3A, 0x3A, 0x46);
/// Passive, de-emphasised text such as status-bar hints.
pub const TEXT_MUTED: Color = Color::Rgb(0x6E, 0x6E, 0x7C);
/// Faintest text: empty states and secondary details.
pub const TEXT_FAINT: Color = Color::Rgb(0x9A, 0x9A, 0xA6);
/// Section-header text.
pub const TEXT_HEADER: Color = Color::Rgb(0x6E, 0x6E, 0x7C);
/// Dimmed cursor label.
pub const CURSOR_DIM: Color = Color::Rgb(0x6C, 0x6A, 0x94);

// ── Accents ──────────────────────────────────────────────────────────────

/// Structural accent: block borders, password markers, passphrase labels.
/// (slate-lavender; more saturated than the surrounding text so borders stay legible)
pub const ACCENT: Color = Color::Rgb(0x6C, 0x6F, 0xA8);
/// Directory names in the file browser. (sky blue)
pub const DIR_NAME: Color = Color::Rgb(0x2E, 0x7D, 0xB0);

// ── Semantics ────────────────────────────────────────────────────────────

/// Success / affirmative (copied, add). (mint)
pub const SUCCESS: Color = Color::Rgb(0x12, 0x87, 0x6A);
/// Warning / edit. (warm gold)
pub const WARNING: Color = Color::Rgb(0xA9, 0x76, 0x1B);
/// Danger / destructive. (soft red)
pub const DANGER: Color = Color::Rgb(0xC0, 0x50, 0x5E);
/// Search / informational (active input). (muted violet)
pub const INFO: Color = Color::Rgb(0xC4, 0xB8, 0xDC);

// ── Status (title bar) ───────────────────────────────────────────────────

/// A blocking load (decrypt / open) is running. (warm gold)
pub const STATUS_LOADING: Color = Color::Rgb(0xA9, 0x76, 0x1B);
/// A save is running. (salmon)
pub const STATUS_SAVING: Color = Color::Rgb(0xC0, 0x6A, 0x55);
/// The latest save attempt failed. (soft red)
pub const STATUS_FAILED: Color = Color::Rgb(0xC0, 0x50, 0x5E);

// ── Inverse ──────────────────────────────────────────────────────────────

/// Foreground drawn on a dark badge or on the inverted cursor. (near-white ink,
/// the light counterpart of Default Dark's near-black `INVERSE_FG`)
pub const INVERSE_FG: Color = Color::Rgb(0xF4, 0xF1, 0xEA);
/// Background of the inverted cursor. (= TEXT)
pub const INVERSE_BG: Color = Color::Rgb(0x3A, 0x3A, 0x46);