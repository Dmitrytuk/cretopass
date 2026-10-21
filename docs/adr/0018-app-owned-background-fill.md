# 0018 — App-owned background fill

The colour scheme ([0015](0015-default-dark-colour-scheme.md)) defined `SURFACE`
and called it "the recommended terminal background": blocks, modals and popups
painted it, but the Tree and Home screens styled text with a foreground only. The
largest area on screen — the tree gutter, the menu gutter, the whole status-bar
row — was therefore still the *terminal's* own background, so the promise that
the UI "renders identically in every terminal" held only for the foreground.

**Decision:** the app owns its background, and that background is its own
surface. One shared renderer, `theming::paint_background`, paints
`ElementStyle::AppBackground` — a dedicated `BASE_BG` role, the surface every
panel sits on — across the full frame at the top of each screen's render, before
any other widget. `ui()` (Tree) and `render()` (Home, which also owns the File
Browser and every modal) each call it exactly once. `SURFACE` goes back to
meaning what its name says: the background of blocks, modals and popups, always a
*different* colour from the canvas.

The scheme puts the canvas above the panels, and nothing in the code depends on
which of the two is lighter:

| Scheme | `BASE_BG` (canvas) | `SURFACE` (panels) | Panels read as |
|--------|--------------------|--------------------|----------------|
| Default Dark | `#2c2c2c` | `#202020` | recessed |

## Consequences

- No cell falls through to the terminal background. Terminal background,
  opacity/transparency and ANSI-palette remapping can no longer change the look,
  so the terminal-independence promise now covers the whole screen.
- A popup is never the same colour as the canvas it covers, so it is always
  distinguishable from the content behind it, whichever direction the scheme
  stacks the two surfaces.
- The status-bar row stays on the canvas (`BASE_BG`) rather than becoming a panel
  strip. The Mode badge and the semantic colours carry the chrome, and the palette
  still gains no separate chrome role; the bottom row reads as the gutter around
  the panel rather than as another panel.
- `Clear` needed no change: every one of its call sites is immediately followed
  by a `Block` with `.style(…)` covering the exact same rect, which re-paints
  `SURFACE` over it. That invariant is now enforced by tests
  (`tree_screen_paints_every_cell`, `search_overlay_paints_every_cell`,
  `home_screen_paints_every_cell`, `file_browser_modal_paints_every_cell`) which
  assert no rendered cell has `Color::Reset` (the terminal default) as its
  background.
- The terminal background is no longer a *requirement* on the user; setting it to
  `BASE_BG` is merely cosmetic now.
- In Default Dark the canvas is a mid grey (`#2c2c2c`) while `TEXT_MUTED`,
  `TEXT_FAINT`, `TEXT_HEADER`, `SELECTION_BG` and `PATH_BAR_BG` were tuned
  against the near-black panel the canvas used to share. Elements drawn straight
  onto the canvas — tree rows, the Home menu, the status-bar hints, the
  selected-row highlight — therefore have less contrast than before, and
  `PATH_BAR_BG` is now darker than the panel it sits in. Fixing that is palette
  re-tuning, not a structural change.

## Rejected alternatives

- **Keep delegating the background to the terminal.** Preserves the user's
  terminal background and transparency, but leaves the app's largest surface
  terminal-dependent and contradicts the TrueColor promise the schemes were
  built on.
- **Paint the terminal itself** (`crossterm` `SetBackgroundColor(Rgb)`), so even
  `Clear` and resizes inherit our colour. Depends on the emulator honouring the
  sequence, and breaks the rule that appearance lives in the ratatui buffer, not
  in the terminal's own colour state.
- **One colour for both the canvas and the panels** (the first cut of this
  change: the canvas simply reused `SURFACE`). Nothing distinguishes a popup from
  the content behind it but its border, which is a regression for any user whose
  terminal background differed from `SURFACE`.
- **A distinct chrome role for the status row.** Still rejected: the row reads as
  chrome through the Mode badge and its hint text, so a third surface colour would
  be vocabulary without a behaviour difference.