//! The **Default Light** colour scheme — the light counterpart of
//! [`palette_default_dark`](super::palette_default_dark), exported as [`VALUES`]
//! and installed by [`theming`](super) when the user selects it. It mirrors that
//! scheme exactly, one colour role each, so `ElementStyle` can drive the same UI
//! from either scheme by swapping which one is active.
//!
//! Every value is a TrueColor RGB colour, so the interface renders identically
//! in every terminal and never depends on the terminal's own ANSI palette.
//!
//! Where Default Dark is a cool mid-grey scheme, Default Light is a warm one: a
//! pale off-white canvas (`base_bg` `#f4f1ea`) with cream panels (`surface`
//! `#e7e2d8`) recessed into it, and a dark ink (`text` `#3a3a46`) for legibility
//! on the light surfaces. The structural accent stays a periwinkle (`accent`
//! `#6c6fa8`), directories stay sky blue (`dir_name` `#2e7db0`), success stays
//! mint (`success` `#12876a`), warning stays warm gold (`warning` `#a9761b`) and
//! information stays muted violet (`info` `#c4b8dc`). As in the dark scheme
//! `base_bg` is painted across the entire screen before anything else, so the
//! terminal's own background never shows through, and `surface` is a *different*
//! colour from the canvas so every block, modal and popup reads as a distinct
//! panel.
//!
//! Unlike Default Dark, the semantic colours here are dark enough to read as
//! foreground text on the light surfaces, so the ink drawn on them is the light
//! `inverse_fg` rather than a near-black ink.
//!
//! Nothing outside [`ElementStyle`](super::ElementStyle) maps these colours onto
//! UI elements.

use ratatui::style::Color;

use super::Palette;

/// The Default Light scheme's colours.
pub const VALUES: Palette = Palette {
    // Surfaces.
    base_bg: Color::Rgb(0xF4, 0xF1, 0xEA),
    surface: Color::Rgb(0xE7, 0xE2, 0xD8),
    selection_bg: Color::Rgb(0xD6, 0xDA, 0xE4),
    path_bar_bg: Color::Rgb(0xD3, 0xCD, 0xC3),

    // Text.
    text: Color::Rgb(0x3A, 0x3A, 0x46),
    text_muted: Color::Rgb(0x6E, 0x6E, 0x7C),
    text_faint: Color::Rgb(0x9A, 0x9A, 0xA6),
    text_header: Color::Rgb(0x6E, 0x6E, 0x7C),
    cursor_dim: Color::Rgb(0x6C, 0x6A, 0x94),

    // Accents.
    accent: Color::Rgb(0x6C, 0x6F, 0xA8),
    dir_name: Color::Rgb(0x2E, 0x7D, 0xB0),

    // Semantics.
    success: Color::Rgb(0x12, 0x87, 0x6A),
    warning: Color::Rgb(0xA9, 0x76, 0x1B),
    danger: Color::Rgb(0xC0, 0x50, 0x5E),
    info: Color::Rgb(0xC4, 0xB8, 0xDC),

    // Status (title bar).
    status_loading: Color::Rgb(0xA9, 0x76, 0x1B),
    status_saving: Color::Rgb(0xC0, 0x6A, 0x55),
    status_failed: Color::Rgb(0xC0, 0x50, 0x5E),

    // Inverse.
    inverse_fg: Color::Rgb(0xF4, 0xF1, 0xEA),
    inverse_bg: Color::Rgb(0x3A, 0x3A, 0x46),
};
