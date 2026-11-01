# 0003 — cursor module

UTF-8-aware text editing primitives operating on **byte offsets** (not grapheme
indices), TUI span rendering helpers, and the masked password fields built on
them.

**Decision:** use byte offsets for cursor position rather than character indices
or grapheme clusters. This avoids O(n) index translation on every keypress. The
cost is that multi-byte character traversal requires `is_char_boundary()` checks
in `cursor_left` and `len_utf8()` lookups in `cursor_right` — both O(1)
operations after the initial slice.

**Rejected alternative:** character indices. Would require scanning the full
string prefix on every cursor movement, which becomes noticeable on long
descriptions.

## Modules

| File | Contents |
|------|----------|
| `editing.rs` | UTF-8-aware byte-offset editing primitives and the TUI span helpers (`cursor_field_spans`, `selected_span`) |
| `secret_field.rs` | The masked password fields (`SecretField`, `SecretFieldGroup`) and their span builders |

## Public API

| Function | Purpose |
|----------|---------|
| `cursor_left(text, cursor) → usize` | Move left one char, landing on a boundary |
| `cursor_right(text, cursor) → usize` | Move right one char |
| `backspace_at(text, cursor) → usize` | Remove the char left of the cursor, return the new offset |
| `delete_at(text, cursor)` | Remove the char under the cursor |
| `insert_char_at(text, cursor, c)` | Insert a char, advancing `cursor` past it |
| `selected_style(is_selected) → Style` | Style for a multi-row selected item |
| `selected_span(text, is_selected) → Span` | Render a list row with the `▸`/pad prefix |
| `cursor_field_spans(label, text, cursor) → Vec<Span>` | Render `text` with an inverted cursor cell under the byte offset |
| `secret_field_spans(field, focused, show) → Vec<Span>` | Masked password value, inverted cursor cell while focused |
| `password_field_spans(label, field, focused, show) → Vec<Span>` | The padded `  Label: ` prefix plus `secret_field_spans` |

`editing` is re-exported `pub`; `secret_field` is re-exported `pub(crate)` — the
password prompts are its only consumers, so the public surface stays the
low-level primitives.

## Editing quirks

- Cursor is a **byte offset**. Editing uses `String::insert(byte_pos, c)` and `String::remove(byte_pos)`.
- Splicing a character shifts the tail of the string, so each edit is O(n) in the text length, unlike the O(1) cursor moves.
- `cursor_field_spans()` splits text at the cursor, renders the character under it as an inverted rectangle (White bg, Black fg), and falls back to a styled space past the end.
- `split_at_cursor()` is the shared `(before, char, after)` decomposition both span builders use.

## Secret fields

- `SecretField` is one masked value plus a byte-offset cursor; it wipes its text
  on drop and redacts it from `Debug`.
- `SecretFieldGroup` holds N fields, one focused index, and one reveal flag. It is
  built with `with_fields(count)` (it has no `Default`) and dispatches the shared
  prompt key map through `apply_action`, reporting a `FieldEdit`.
- `pair_verdict()` validates the first two fields and returns
  `Option<PairVerdict>`: `None` when the group is not a pair, `Empty` when both
  fields are empty, `Mismatch` (which resets the cursors) when they differ, and
  `Match` otherwise.
- **Mask length is one `*` per byte**, not per character, so the byte-offset
  cursor stays aligned with the mask for multi-byte input.

## Dependencies

- `theming` (`ElementStyle`, `Icon`) — for cursor/selection styles
- `zeroize` — wiping the password text on drop
- `ratatui` 0.28 — for `Span`, `Style`
