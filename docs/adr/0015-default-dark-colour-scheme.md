# 0015 — Default Dark colour scheme

**Status:** partially superseded by [0019](0019-runtime-colour-scheme-switching.md)
— the app now ships a second scheme (**Default Light**) and switches between them
at runtime from the Configuration modal. The design below — one named TrueColor
scheme, drawn only through `ElementStyle` — still stands; only the compile-time
single-scheme selection and the `pub use … as palette` re-export are gone.

The UI drew every colour from the 16-colour ANSI palette. ANSI slots are
re-mapped by each terminal's own theme, so `Blue`, `DarkGray` and `Gray`
rendered as dark, low-contrast text on a black background: in Windows
PowerShell and other dark terminals the file-browser directory names,
empty-state text, section headers, the cursor label and the status-bar hints
were effectively invisible, while the same build looked fine in Alacritty,
whose theme assigns brighter values to those slots. The app looked different —
and sometimes broken — in every terminal.

**Decision:** define one named, terminal-independent colour scheme, **Default
Dark**, as TrueColor RGB constants in `theming/palette_default_dark.rs`, and
route every UI colour through it. `ElementStyle`
([ADR 0004](../src/theming/docs/adr/0004-theming.md)) is the only thing that maps
those colours onto UI elements; no module defines a colour inline. The scheme
started from the VS Code *Astro Dark* theme, but its surfaces have been re-tuned
to mid greys: a light canvas (`#2c2c2c`) with darker panels (`#202020`) recessed
into it.

The scheme was exposed as `palette` by a single re-export line in `theming.rs`:

```rust
pub use palette_default_dark as palette;
```

`use super::palette;` in `style.rs` and `crate::theming::palette` elsewhere
resolved through this re-export, so `ElementStyle` and every consumer stayed
untouched. [ADR 0019](0019-runtime-colour-scheme-switching.md) replaced that
re-export: the scheme is now a `Palette` value held in an active-scheme cell, read
through `theming::palette()`.

Earlier revisions of this module shipped two further schemes — **Default**, a
cool near-black-indigo scheme with a cyan accent, and **Cyber Orchid**, a warm
scheme with a purple-blue accent. Both have since been removed; Default Dark is
the only scheme.

## Palette

Every value is taken from the VS Code *Astro Dark* theme, mapped onto the scheme's
roles, except the surface pair (`BASE_BG` / `SURFACE`), which has been re-tuned to
mid greys.

| Role | Value | Notes |
|------|-------|-------|
| `BASE_BG` | `#2c2c2c` | canvas; a mid grey, lighter than `SURFACE` |
| `SURFACE` | `#202020` | block / modal / popup background; recessed below `BASE_BG` |
| `SELECTION_BG` | `#3a3d4e` | selection over `SURFACE`, brightened for badge / row legibility |
| `PATH_BAR_BG` | `#13151a` | darker than both `SURFACE` and `SELECTION_BG` |
| `TEXT` | `#b0b0be` | editor foreground, brightened for legibility |
| `TEXT_MUTED` / `TEXT_HEADER` | `#686974` | comments |
| `TEXT_FAINT` | `#43444d` | whitespace guides |
| `CURSOR_DIM` | `#817c9c` | muted grey-purple |
| `ACCENT` | `#9ea1c8` | slate-lavender, more saturated for border legibility |
| `DIR_NAME` | `#5cc9f5` | sky blue |
| `SUCCESS` | `#23ebc0` | mint |
| `WARNING` | `#ffd493` | warm gold |
| `DANGER` / `STATUS_FAILED` | `#e06c75` | soft red |
| `INFO` | `#6c5e8a` | muted violet |
| `STATUS_LOADING` | `#ffd493` | warm gold |
| `STATUS_SAVING` | `#ff9e93` | salmon |
| `INVERSE_FG` / `INVERSE_BG` | `#0f1014` / `#b0b0be` | near-black ink / `TEXT` |

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **TrueColor RGB, not ANSI.** Every colour is an explicit `Color::Rgb` value. | The terminal cannot re-map RGB, so the app renders identically everywhere. |
| 2 | **One named scheme in `theming/palette_default_dark.rs`.** | A single place to change how the app looks; `ElementStyle` stays the semantic mapping layer. |
| 3 | **Brighten the status glyphs and drop their bold.** Loading/Saving/SaveFailed use bright RGB without `Modifier::BOLD`. | The spinner was meant to draw the eye; bold muddied the glyph and the label. |
| 4 | **Hints are `TEXT_MUTED` (`#686974`).** | Visible on the surface, but quieter than body text so hints never steal focus from the mode badge. |
| 5 | **Mode-badge colour coding is unchanged.** Destructive red, attention gold, active input violet, add mint, passive selection. | [ADR 0014](0014-status-bar-unification.md) made badge colour a cross-screen vocabulary; the scheme preserves it. |
| 6 | **Overlays are painted `SURFACE`, not `Black`.** | On a black terminal an ANSI-black panel was invisible; the mid-grey surfaces read as distinct panels. `BASE_BG` is painted across every screen ([0018](0018-app-owned-background-fill.md)). |
| 7 | **The canvas is lighter than the panels.** `BASE_BG` `#2c2c2c` sits above `SURFACE` `#202020`. | Blocks, modals and popups then read as recessed into the canvas, keeping the two surfaces distinct whichever way they are stacked. |

## Consequences

- The app no longer depends on the terminal's palette: the same colours render
  in PowerShell, Alacritty, Kitty, Windows Terminal and VSCode.
- `theming` stays a strict leaf: `palette_default_dark.rs` depends only on
  `ratatui`, and `ElementStyle` is still the only consumer.
- The `status_bar` unit test that asserted the `CUT` badge was `Color::Red` now
  asserts the active scheme's `danger` colour, so the badge test tracks the scheme
  instead of a palette slot.
- Every screen paints `BASE_BG` across itself as its Base background, and the
  overlays and modals use `SURFACE` ([0018](0018-app-owned-background-fill.md)),
  so the terminal's own background no longer shows anywhere.
- Default Dark was the only scheme; the earlier Default and Cyber Orchid schemes
  were removed, so `theming.rs` no longer carried a commented-out `pub use` line
  per alternative. [ADR 0019](0019-runtime-colour-scheme-switching.md) later added
  a second maintained scheme, **Default Light**, selected at runtime rather than
  at compile time.

## Rejected alternatives

- **Keep ANSI and only swap the dark slots for bright ones.** Still
  terminal-dependent; exact hues would keep drifting per theme.
- **Inline RGB values in each `ElementStyle` arm.** Scatters the scheme across
  seventy arms with no single place to read or change it.
- **Multiple schemes selected at compile time.** Earlier revisions shipped up to
  three schemes behind a commented `pub use` line each; they were dropped in
  favour of one maintained scheme, so no inactive module needs to stay compiling.
- **Runtime theme switching / a registry.** Rejected at the time as more
  machinery (state, config, or a picker) than the requirement asked for; later
  revisited and adopted in [ADR 0019](0019-runtime-colour-scheme-switching.md).