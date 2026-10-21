//! The **Default Dark** colour scheme — one of the two schemes the app can
//! switch between, exported as [`VALUES`] and installed by [`theming`](super)
//! when the user selects it. It defines every colour role, so
//! [`ElementStyle`](super::ElementStyle) maps them onto UI elements directly.
//!
//! Every value is a TrueColor RGB colour, so the interface renders identically
//! in every terminal and never depends on the terminal's own ANSI palette.
//!
//! The scheme started from the *VS Code Astro Dark* theme, but its surfaces have
//! been re-tuned to mid greys: a light canvas (`#2c2c2c`) with darker panels
//! (`#202020`) recessed into it. The rest is derived from that theme — a brighter
//! grey-blue foreground (`#b0b0be`), a periwinkle structural accent (`#9ea1c8`),
//! sky-blue directories (`#5cc9f5`), mint success (`#23ebc0`), warm-gold warning
//! (`#ffd493`) and muted-violet information (`#6c5e8a`). `base_bg` (`#2c2c2c`) is
//! painted across the entire screen before anything else, so the terminal's own
//! background never shows through; `surface` (`#202020`) sits *below* it, so
//! every block, modal and popup reads as recessed into the canvas.
//!
//! Nothing outside [`ElementStyle`](super::ElementStyle) maps these colours onto
//! UI elements.

use ratatui::style::Color;

use super::Palette;

/// The Default Dark scheme's colours.
pub const VALUES: Palette = Palette {
    // Surfaces.
    base_bg: Color::Rgb(0x2C, 0x2C, 0x2C),
    surface: Color::Rgb(0x20, 0x20, 0x20),
    selection_bg: Color::Rgb(0x3A, 0x3D, 0x4E),
    path_bar_bg: Color::Rgb(0x13, 0x15, 0x1A),

    // Text.
    text: Color::Rgb(0xB0, 0xB0, 0xBE),
    text_muted: Color::Rgb(0x68, 0x69, 0x74),
    text_faint: Color::Rgb(0x43, 0x44, 0x4D),
    text_header: Color::Rgb(0x68, 0x69, 0x74),
    cursor_dim: Color::Rgb(0x81, 0x7C, 0x9C),

    // Accents.
    accent: Color::Rgb(0x9E, 0xA1, 0xC8),
    dir_name: Color::Rgb(0x5C, 0xC9, 0xF5),

    // Semantics.
    success: Color::Rgb(0x23, 0xEB, 0xC0),
    warning: Color::Rgb(0xFF, 0xD4, 0x93),
    danger: Color::Rgb(0xE0, 0x6C, 0x75),
    info: Color::Rgb(0x6C, 0x5E, 0x8A),

    // Status (title bar).
    status_loading: Color::Rgb(0xFF, 0xD4, 0x93),
    status_saving: Color::Rgb(0xFF, 0x9E, 0x93),
    status_failed: Color::Rgb(0xE0, 0x6C, 0x75),

    // Inverse.
    inverse_fg: Color::Rgb(0x0F, 0x10, 0x14),
    inverse_bg: Color::Rgb(0xB0, 0xB0, 0xBE),
};
