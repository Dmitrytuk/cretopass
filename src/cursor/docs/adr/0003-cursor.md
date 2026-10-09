# 0003 — cursor module

UTF-8-aware text editing primitives operating on **byte offsets** (not grapheme
indices) and TUI span rendering helpers.

**Decision:** use byte offsets for cursor position rather than character indices
or grapheme clusters. This avoids O(n) index translation on every keypress. The
cost is that multi-byte character traversal requires `is_char_boundary()` checks
in `cursor_left` and `len_utf8()` lookups in `cursor_right` — both O(1)
operations after the initial slice.

**Rejected alternative:** character indices. Would require scanning the full
string prefix on every cursor movement, which becomes noticeable on long
descriptions.

## Public API

| Function | Purpose |
|----------|---------|
| `cursor_left(text, cursor) → usize` | Move left one char, landing on a boundary |
| `cursor_right(text, cursor) → usize` | Move right one char |
| `backspace_at(text, cursor) → (String, usize)` | Remove char left of cursor |
| `delete_at(text, cursor) → (String, usize)` | Remove char under cursor |
| `insert_char_at(text, cursor, c) → (String, usize)` | Insert char, advance cursor |
| `selected_style() → Style` | Style for multi-row selected item |
| `selected_span(text, style) → Span` | Render span with `▸` prefix for selected row |
| `cursor_field_spans(text, cursor, has_focus, is_password) → Vec<Span>` | Render text with inverted cursor block; if `is_password` and unfocused, show `*****` |

## Editing quirks

- Cursor is a **byte offset**. Editing uses `String::insert(byte_pos, c)` and `String::remove(byte_pos)`.
- `cursor_field_spans()` splits text at cursor, renders the character under cursor as inverted rectangle (White bg, Black fg).
- Password masking: when `has_focus == false` and `is_password == true`, the entire field renders as `*****`.

## Dependencies

- `theming` (`ElementStyle`, `Icon`) — for selected/highlight styles
- `ratatui` 0.28 — for `Span`, `Style`