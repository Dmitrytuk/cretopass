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
the surface every panel sits on.
_Avoid_: Canvas, wallpaper, terminal background

**Default Dark colour scheme** / **Default Light colour scheme**:
The two named TrueColor RGB palettes (`theming/palette_default_dark.rs`,
`theming/palette_default_light.rs`) from which every UI colour is drawn. Neither
uses the terminal's re-mappable ANSI slots, so the UI renders identically in every
terminal. `ElementStyle` is the only thing that maps these colours onto UI
elements. Default Dark is a cool mid-grey scheme derived from the VS Code Astro
Dark theme; Default Light is its warm off-white counterpart.
_Avoid_: Theme, colours, style

**Active colour scheme**:
The one scheme the app is currently rendering with, held in a process-wide cell
and swapped at runtime from the Home Configuration modal. The choice is persisted
as `palette_name`; Default Dark is active until the user picks another.
_Avoid_: Current theme, selected palette

**Mouse capture**:
The terminal mode in which the app, not the terminal, receives the wheel and
clicks. The `terminal` module enables it at startup and disables it symmetrically
on exit, error and panic. The trade-off is that the terminal's native text
selection is off while it is on (most emulators still select while `Shift` is
held).
_Avoid_: Mouse mode, mouse reporting

**Wheel**:
A scroll tick that maps to Up/Down navigation through the existing key dispatch,
so every list scrolls identically in every terminal with no coordinate maths. It
never dismisses a dismiss-on-any-key overlay (Help, About, Alert).
_Avoid_: Scroll, mouse scroll

**Hit-box**:
A rectangular on-screen region a renderer records, paired with the Click target it
stands for. The `hitbox` leaf resolves a click's cell to the **topmost** recorded
box, so a modal's rows outrank the screen rows drawn behind them.
_Avoid_: Click region, hit area, rectangle

**Click**:
A left/right button press that a screen hit-tests on its Hit-boxes and turns into
the same `Action` its key would produce. A click first moves the selection to the
clicked row; a repeat click on the already-selected row activates it. The right
button copies a Detail field's value. Drags and moves are ignored, and a
destructive confirmation has no Hit-boxes at all, so no click can answer `y`.
_Avoid_: Tap, press
