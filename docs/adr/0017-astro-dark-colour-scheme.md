# 0017 — Astro Dark colour scheme

The UI already offered two schemes: **Default** ([ADR 0015](0015-default-color-scheme.md))
and **Cyber Orchid** ([ADR 0016](0016-cyber-orchid-colour-scheme.md)). We wanted
a third option for users coming from the VS Code *Astro Dark* theme, and Astro
Dark became the active scheme.

**Decision:** add a third, equally complete scheme, **Astro Dark**, in
`theming/palette_astro_dark.rs`, and make it the active one. It exposes the exact
same public constants as the other schemes, so `ElementStyle`
([ADR 0004](../src/theming/docs/adr/0004-theming.md)) needs no change. The active
scheme is chosen at compile time by a single re-export line in `theming.rs`:

```rust
pub use palette_astro_dark as palette;  // Astro Dark (active)
// pub use palette_cyber_orchid as palette;
// pub use palette_default as palette;
```

## Palette

Every value is taken from the VS Code *Astro Dark* theme, mapped onto the scheme's
roles, except the surface pair (`BASE_BG` / `SURFACE`), which has since been
re-tuned to mid greys.

| Role | Value | Source |
|------|-------|--------|
| `BASE_BG` | `#2c2c2c` | canvas; re-tuned to a mid grey, lighter than `SURFACE` |
| `SURFACE` | `#202020` | block / modal / popup background; re-tuned to a mid grey, recessed below `BASE_BG` |
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

## Consequences

- Both other schemes still compile; the inactive ones are `pub`, so no dead-code
  warning. Astro Dark is the scheme that ships.
- Switching schemes is a one-line edit plus a rebuild; no other file changes.
- The surface pair is a deviation from the source theme, and Astro Dark is the
  only scheme whose canvas is lighter than its panels
  ([ADR 0018](0018-app-owned-background-fill.md)), so panels here read as
  recessed rather than raised.