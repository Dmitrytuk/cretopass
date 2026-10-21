# 0016 — Cyber Orchid colour scheme

The UI had a single colour scheme, **Default** ([ADR 0015](0015-default-color-scheme.md)):
a cool, near-black-indigo scheme with a cyan accent. We want to try a warmer,
more saturated look — deep indigo surfaces, a vivid purple-blue accent, magenta
attention and raspberry danger — without disturbing the Default look or adding
any user-facing theme UI.

**Decision:** add a second, equally complete scheme, **Cyber Orchid**, in
`theming/palette_cyber_orchid.rs`. It exposes the exact same public constants as
`theming/palette_default.rs`, so `ElementStyle` ([ADR 0004](../src/theming/docs/adr/0004-theming.md))
needs no change. The active scheme is chosen at compile time by a single
re-export line in `theming.rs`:

```rust
pub use palette_default as palette;      // Default (active)
// pub use palette_cyber_orchid as palette;  // Cyber Orchid
```

`use super::palette;` in `style.rs` and `crate::theming::palette` elsewhere
resolve through this re-export, so switching the line re-skins the whole app.

## Palette

Built from the *Cyber Orchid* palette (Supreme Indigo `#331a84`, Beyond Blue
`#6a26f2`, Deep White `#f5f1e6`, Awesome `#e941fb`, Raspberry `#e8114e`); the
remaining roles are tints in the same gamut.

| Role | Value | Notes |
|------|-------|-------|
| `SURFACE` | `#331a84` | Supreme Indigo |
| `SELECTION_BG` | `#4a2498` | Lightened surface |
| `PATH_BAR_BG` | `#3f1e8a` | Surface nuance |
| `TEXT` | `#f5f1e6` | Deep White |
| `TEXT_MUTED` | `#9a8bc4` | Tint |
| `TEXT_FAINT` | `#6b5c94` | Tint |
| `TEXT_HEADER` | `#5a4a80` | Tint |
| `CURSOR_DIM` | `#7a6ba0` | Tint |
| `ACCENT` | `#6a26f2` | Beyond Blue |
| `DIR_NAME` | `#8a5af5` | Lightened accent |
| `SUCCESS` | `#5ec49e` | Mint, complementary |
| `WARNING` | `#d4a84b` | Warm amber |
| `DANGER` | `#e8114e` | Raspberry |
| `INFO` | `#e941fb` | Awesome (magenta) |
| `STATUS_LOADING` | `#5ce0a8` | Mint-cyan |
| `STATUS_SAVING` | `#f0c84a` | Warm gold |
| `STATUS_FAILED` | `#e8114e` | Raspberry |
| `INVERSE_FG` / `INVERSE_BG` | `#331a84` / `#f5f1e6` | = SURFACE / TEXT |

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Same constants, second module.** `palette_cyber_orchid.rs` mirrors every name in `palette_default.rs`. | `ElementStyle` and every consumer stay untouched; the schemes are drop-in interchangeable. |
| 2 | **Compile-time switch via one `pub use … as palette` line.** | No runtime registry, no new dependency, no state to thread through the app — matches the "no theme UI" requirement. |
| 3 | **Missing roles derived as tints of the five source colours.** | The palette has five colours but the scheme has ~20 roles; green/amber are added as complementary hues so success/warning stay legible on the indigo surface. |
| 4 | **`INFO` takes Awesome (`#e941fb`), `DANGER`/`STATUS_FAILED` take Raspberry.** | Preserves the cross-screen badge vocabulary of [ADR 0014](0014-status-bar-unification.md) while giving the cyber look its magenta attention colour. |

## Consequences

- Both schemes always compile; the inactive one is `pub`, so it raises no dead-code
  warning.
- Switching schemes is a one-line edit plus a rebuild; no other file changes.
- Docs that referenced `theming/palette.rs` now name `theming/palette_default.rs`.

## Rejected alternatives

- **Runtime theme switching / a registry.** More machinery (state, config, or a
  picker) than the requirement asks for; the task explicitly rules out UI.
- **A Cargo feature flag per scheme.** Heavier than a single source line and
  invites feature-combination bugs for no benefit.
- **Editing the Default palette in place.** Would change the shipping look
  instead of offering an alternative.