# 0015 — Default colour scheme

The UI drew every colour from the 16-colour ANSI palette. ANSI slots are
re-mapped by each terminal's own theme, so `Blue`, `DarkGray` and `Gray`
rendered as dark, low-contrast text on a black background: in Windows
PowerShell and other dark terminals the file-browser directory names,
empty-state text, section headers, the cursor label and the status-bar hints
were effectively invisible, while the same build looked fine in Alacritty,
whose theme assigns brighter values to those slots. The app looked different —
and sometimes broken — in every terminal.

**Decision:** define one named, terminal-independent colour scheme, **Default**,
as TrueColor RGB constants in `theming/palette_default.rs`, and route every UI
colour through it. `ElementStyle` ([ADR 0004](../src/theming/docs/adr/0004-theming.md))
is the only thing that maps those colours onto UI elements; no module defines a
colour inline. The scheme is cool-toned: near-black indigo surfaces, soft
off-white text, a cyan structural accent, and brighter semantic accents.

A second scheme, **Cyber Orchid**, lives alongside it in
`theming/palette_cyber_orchid.rs` and re-tunes the same roles to a warmer palette
([ADR 0016](0016-cyber-orchid-colour-scheme.md)).

## Palette

| Role | Value | Used by |
|------|-------|---------|
| `SURFACE` | `#1a1b2e` | Block / modal / popup background (was ANSI `Black`) |
| `SELECTION_BG` | `#3a4055` | Selected, matched and focused rows; neutral mode badges (was ANSI `DarkGray`) |
| `PATH_BAR_BG` | `#3b4261` | File-browser path bar |
| `TEXT` | `#d7dae0` | Primary text (was ANSI `White`) |
| `TEXT_MUTED` | `#9aaec9` | Status-bar hints (was ANSI `Gray`) |
| `TEXT_FAINT` | `#6b7080` | Empty states, secondary details (was ANSI `DarkGray`) |
| `TEXT_HEADER` | `#586078` | Section headers (was ANSI `DarkGray`) |
| `CURSOR_DIM` | `#7a85a0` | Dimmed cursor label (was ANSI `DarkGray`) |
| `ACCENT` | `#56b6c2` | Block borders, password markers, passphrase labels (was ANSI `Cyan`) |
| `DIR_NAME` | `#82aaff` | Directory names in the file browser (was ANSI `Blue`) |
| `SUCCESS` | `#98c379` | Copied / add badges and borders (was ANSI `Green`) |
| `WARNING` | `#e5c07b` | Edit / change-password badges (was ANSI `Yellow`) |
| `DANGER` | `#e05555` | Cut / confirm-delete / error badges and borders (was ANSI `Red`) |
| `INFO` | `#4a76b8` | Search / field-type / custom-name badges (was ANSI `Blue`) |
| `STATUS_LOADING` | `#7cfc00` | Title-bar "Loading" glyph and label |
| `STATUS_SAVING` | `#ffd700` | Title-bar "Saving" glyph and label |
| `STATUS_FAILED` | `#ff4444` | Title-bar "Save failed" label |
| `INVERSE_FG` / `INVERSE_BG` | `#1a1b2e` / `#d7dae0` | Inverted cursor; foreground on bright badges |

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **TrueColor RGB, not ANSI.** Every colour is an explicit `Color::Rgb` value. | The terminal cannot re-map RGB, so the app renders identically everywhere. |
| 2 | **One named scheme per module in `theming/palette_default.rs`.** | A single place to change how the app looks; `ElementStyle` stays the semantic mapping layer. A second scheme lives in its own module ([ADR 0016](0016-cyber-orchid-colour-scheme.md)). |
| 3 | **Brighten the status glyphs and drop their bold.** Loading/Saving/SaveFailed use bright RGB without `Modifier::BOLD`. | The spinner was meant to draw the eye; bold muddied the glyph and the label. |
| 4 | **Hints are `TEXT_MUTED` (`#9aaec9`).** | Visible on the surface, but quieter than body text so hints never steal focus from the mode badge. |
| 5 | **Mode-badge colour coding is unchanged.** Destructive red, attention yellow, active input blue, generator cyan, add green, passive selection. Only the concrete shades move to RGB. | [ADR 0014](0014-status-bar-unification.md) made badge colour a cross-screen vocabulary; the scheme preserves it. |
| 6 | **Overlays are painted `SURFACE`, not `Black`.** | On a black terminal an ANSI-black panel was invisible; indigo surfaces read as distinct panels. SURFACE doubles as the recommended terminal background. |

## Consequences

- The app no longer depends on the terminal's palette: the same colours render
  in PowerShell, Alacritty, Kitty, Windows Terminal and VSCode.
- `theming` stays a strict leaf: `palette_default.rs` depends only on `ratatui`,
  and `ElementStyle` is still the only consumer.
- The `status_bar` unit test that asserted the `CUT` badge was `Color::Red` now
  asserts `palette::DANGER`, so the badge test tracks the scheme instead of a
  palette slot.
- Only overlays and modals paint `SURFACE`; the main Tree and Home screens keep
  the terminal's own background, so a user who wants the full look sets their
  terminal background to `#1a1b2e`.

## Rejected alternatives

- **Keep ANSI and only swap the dark slots for bright ones.** Still
  terminal-dependent; exact hues would keep drifting per theme.
- **Inline RGB values in each `ElementStyle` arm.** Scatters the scheme across
  seventy arms with no single place to read or change it.
- **Runtime theme switching.** When this scheme was defined there was no second
  theme yet; a registry would have been premature, so the named module left room
  to add one later. The second scheme ([ADR 0016](0016-cyber-orchid-colour-scheme.md))
  is still selected at compile time, not run time.