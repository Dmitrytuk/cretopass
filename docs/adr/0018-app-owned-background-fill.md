# 0018 — App-owned background fill

The colour schemes ([0015](0015-default-color-scheme.md),
[0016](0016-cyber-orchid-colour-scheme.md),
[0017](0017-astro-dark-colour-scheme.md)) defined `SURFACE` and called it "the
recommended terminal background": blocks, modals and popups painted it, but the
Tree and Home screens styled text with a foreground only. The largest area on
screen — the tree gutter, the menu gutter, the whole status-bar row — was
therefore still the *terminal's* own background, so the promise that the UI
"renders identically in every terminal" held only for the foreground.

**Decision:** the app owns its background. One shared renderer,
`theming::paint_background`, paints `ElementStyle::AppBackground` — the active
scheme's [`SURFACE`] — across the full frame at the top of each screen's render,
before any other widget. `ui()` (Tree) and `render()` (Home, which also owns the
File Browser and every modal) each call it exactly once. No new colour role is
introduced: `SURFACE` becomes both the Base background and the panel colour.

## Consequences

- No cell falls through to the terminal background. Terminal background,
  opacity/transparency and ANSI-palette remapping can no longer change the look,
  so the terminal-independence promise now covers the whole screen.
- The status-bar row is `SURFACE` too. The Mode badge and the semantic colours
  carry the chrome; the palette deliberately gains no separate chrome role.
- `Clear` needed no change: every one of its call sites is immediately followed
  by a `Block` with `.style(…)` covering the exact same rect, which re-paints
  `SURFACE` over it. That invariant is now enforced by tests
  (`tree_screen_paints_every_cell`, `search_overlay_paints_every_cell`,
  `home_screen_paints_every_cell`, `file_browser_modal_paints_every_cell`) which
  assert no rendered cell has `Color::Reset` (the terminal default) as its
  background.
- `SURFACE` is no longer a *requirement* on the terminal; setting the terminal
  background to it is merely cosmetic now.

## Rejected alternatives

- **Keep delegating the background to the terminal.** Preserves the user's
  terminal background and transparency, but leaves the app's largest surface
  terminal-dependent and contradicts the TrueColor promise the schemes were
  built on.
- **Paint the terminal itself** (`crossterm` `SetBackgroundColor(Rgb)`), so even
  `Clear` and resizes inherit our colour. Depends on the emulator honouring the
  sequence, and breaks the rule that appearance lives in the ratatui buffer, not
  in the terminal's own colour state.
- **A dedicated `BASE_BG` role, or a distinct chrome shade for the status row.**
  Both would be synonyms of `SURFACE` today, adding vocabulary with no behaviour
  difference. Splitting them later is a one-line change if a canvas ever needs to
  differ from the panels.