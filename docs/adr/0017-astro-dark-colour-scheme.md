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
roles.

| Role | Value | Source |
|------|-------|--------|
| `SURFACE` | `#0f1014` | editor background |
| `SELECTION_BG` | `#1a1b24` | selection over `SURFACE` |
| `PATH_BAR_BG` | `#13151a` | between the two |
| `TEXT` | `#9898a2` | editor foreground |
| `TEXT_MUTED` / `TEXT_HEADER` | `#686974` | comments |
| `TEXT_FAINT` | `#43444d` | whitespace guides |
| `CURSOR_DIM` | `#817c9c` | muted grey-purple |
| `ACCENT` | `#acaaff` | periwinkle |
| `DIR_NAME` | `#5cc9f5` | sky blue |
| `SUCCESS` | `#23ebc0` | mint |
| `WARNING` | `#ffd493` | warm gold |
| `DANGER` / `STATUS_FAILED` | `#eef0f9` | near-white |
| `INFO` | `#da68fb` | magenta |
| `STATUS_LOADING` | `#23ebc0` | mint |
| `STATUS_SAVING` | `#ffd493` | warm gold |
| `INVERSE_FG` / `INVERSE_BG` | `#0f1014` / `#9898a2` | = SURFACE / TEXT |

## Consequences

- Both other schemes still compile; the inactive ones are `pub`, so no dead-code
  warning. Astro Dark is the scheme that ships.
- Switching schemes is a one-line edit plus a rebuild; no other file changes.