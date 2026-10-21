# 0019 — Runtime colour-scheme switching

[ADR 0015](0015-default-dark-colour-scheme.md) deliberately shipped **one**
colour scheme, selected at compile time by `pub use palette_default_dark as
palette;`, and explicitly listed *runtime theme switching / a registry* among its
rejected alternatives. The app now ships two schemes — **Default Dark** and
**Default Light** — and lets the user switch between them from the Home
**Configuration** modal, so that compile-time-only decision is reversed here.

**Decision:** both schemes are plain `Palette` values
(`palette_default_dark::VALUES`, `palette_default_light::VALUES`) defined in
[`theming`](0004-theming.md). The active scheme lives in one process-wide
`RwLock<Palette>` cell, seeded with Default Dark, read through `theming::palette()`
and replaced through `theming::set_palette()`. `ElementStyle::style()` resolves
against the active scheme, so no signature anywhere else in the UI changes: a
scheme swap is a single write, and the next frame renders in the new colours.
`Palette` is `Copy` (~80 bytes), so callers take a cheap snapshot rather than
borrowing the lock.

The choice is persisted as `Configuration::palette_name`
([vault 0006](../../src/vault/docs/adr/0006-vault-module.md)) and applied in
`HomeState::new()` before the first frame. In the Configuration modal the scheme
is a third row — a cyclic choice rendered as `Colour scheme: Dark|Light`, toggled
with `Space` — not a checkbox. Like the other settings, `Space` applies the change
immediately (so the user sees it live) while `Enter` persists and `Esc` reverts.

## Why a global cell, not parameter threading

Passing `&Palette` into `ElementStyle::style()` would have touched roughly eighty
call sites across `tree`, `vault`, and `file_browser`, none of which otherwise care
about colour. A process-wide cell keeps the render functions signature-free and
confines scheme knowledge to `theming`, which stays a strict leaf: the cell
depends only on `std` and `ratatui`.

## Consequences

- `theming.rs` owns the `Palette` type, the scheme-name constants
  (`SCHEME_DARK`, `SCHEME_LIGHT`), and the accessors; the scheme files only hold
  the `VALUES` of each scheme.
- The `palette` re-export and `use super::palette;` module alias from ADR 0015 are
  gone; `palette()` is now a function returning the active scheme.
- Tests that assert a rendered colour compare against `theming::palette()` (the
  active scheme) rather than a fixed constant, so they hold under either scheme.
- `Configuration` gains `palette_name`; a config written by an older build fails
  to deserialize and falls back to defaults, as `load_config` already handles.

## Rejected alternatives

- **Compile-time selection with two `pub use` lines.** Cannot switch without a
  rebuild and would need the whole UI re-threaded anyway to observe the change.
- **Threading `&Palette` through every render call.** Correct but case-by-case
  invasive; the palette is cross-cutting state, not a per-widget input.
- **An enum in `Configuration`.** `rkyv` serialization of enums added format risk
  for very little gain; the canonical name string is stable and extensible.