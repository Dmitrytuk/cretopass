# Theming

Centralized presentation layer: UI strings, styles, icons, key bindings, and the
shared status-bar and Base background renderers. Depends on nothing else in the
project.

## Language

**Status bar**:
The single line of chrome at the bottom of every screen: the active mode as a
Mode badge followed by Hint text. Exactly one is drawn per screen. A modal
embedded in a screen — the File Browser — draws none of its own; the owning
screen renders its parts.
_Avoid_: Footer, help bar, options bar

**Mode badge**:
The filled `[MODE]` label leading the Status bar. Its colour encodes the risk or
importance of the active action, and the same colour means the same thing in
every screen.
_Avoid_: Mode indicator, mode chip, mode tag

**Hint**:
The passive key binding text shown after the Mode badge on the Status bar. It
describes the keys available in the current mode.
_Avoid_: Help text, keybinding bar, help bar

**Base background**:
The colour the app paints across the entire screen before drawing anything, so
no cell ever falls through to the terminal's own background. One shared renderer
does this for every screen, and the colour is the active scheme's `BASE_BG` —
the darkest surface, below the `SURFACE` panels.
_Avoid_: Canvas, wallpaper, terminal background

**Default colour scheme**:
The named TrueColor RGB palette (`theming/palette_default.rs`) from which every
UI colour is drawn. It never uses the terminal's re-mappable ANSI slots, so the
UI renders identically in every terminal. `ElementStyle` is the only thing that
maps these colours onto UI elements. Alternatives re-tune the same roles:
**Cyber Orchid** (`theming/palette_cyber_orchid.rs`) is a warm scheme, and
**Astro Dark** (`theming/palette_astro_dark.rs`) is a cold scheme derived from
the VS Code Astro Dark theme. The active one is chosen by the `pub use … as
palette` line in `theming.rs` — currently Astro Dark.
_Avoid_: Theme, colours, style