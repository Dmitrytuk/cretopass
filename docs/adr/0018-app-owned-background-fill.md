# 0018 — App-owned background fill

The colour schemes ([0015](0015-default-color-scheme.md),
[0016](0016-cyber-orchid-colour-scheme.md),
[0017](0017-astro-dark-colour-scheme.md)) defined `SURFACE` and called it "the
recommended terminal background": blocks, modals and popups painted it, but the
Tree and Home screens styled text with a foreground only. The largest area on
screen — the tree gutter, the menu gutter, the whole status-bar row — was
therefore still the *terminal's* own background, so the promise that the UI
"renders identically in every terminal" held only for the foreground.

**Decision:** the app owns its background, and that background is its own
surface. One shared renderer, `theming::paint_background`, paints
`ElementStyle::AppBackground` — a dedicated `BASE_BG` role, the darkest surface in
the scheme — across the full frame at the top of each screen's render, before any
other widget. `ui()` (Tree) and `render()` (Home, which also owns the File
Browser and every modal) each call it exactly once. `SURFACE` goes back to
meaning what its name says: the background of blocks, modals and popups, one step
brighter than the canvas. Every scheme keeps the elevation order
`BASE_BG` < `SURFACE` < `PATH_BAR_BG` < `SELECTION_BG`.

| Scheme | `BASE_BG` | `SURFACE` |
|--------|-----------|-----------|
| Default | `#141526` | `#1a1b2e` |
| Cyber Orchid | `#303030` | `#3a3a3a` |
| Astro Dark | `#0a0b0e` | `#0f1014` |

## Consequences

- No cell falls through to the terminal background. Terminal background,
  opacity/transparency and ANSI-palette remapping can no longer change the look,
  so the terminal-independence promise now covers the whole screen.
- Panels read as raised: modals and popups keep `SURFACE` while sitting on the
  darker `BASE_BG`, so they are no longer the same colour as the canvas they cover.
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