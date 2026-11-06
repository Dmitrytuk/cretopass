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
| `strip_control_chars(text) → Cow<str>` | Drop every control character (single-line fields) |
| `PastePolicy::clean(text) → Cow<str>` | Sanitise a paste: `SingleLine` drops control chars, `Digits` keeps ASCII digits |
| `selected_style(is_selected) → Style` | Style for a multi-row selected item |
| `selected_span(text, is_selected) → Span` | Render a list row with the `▸`/pad prefix |
| `cursor_field_spans(label, text, cursor) → Vec<Span>` | Render `text` with an inverted cursor cell under the byte offset |
| `secret_field_spans(field, focused, show) → Vec<Span>` | Masked password value, inverted cursor cell while focused |
| `password_field_spans(label, field, focused, show) → Vec<Span>` | The padded `  Label: ` prefix plus `secret_field_spans` |

`editing` is re-exported `pub`; `secret_field` is re-exported `pub(crate)` — the
password prompts are its only consumers, so the public surface stays the
low-level primitives. `strip_control_chars` is `pub(crate)`: only the paste
policy calls it.

## Sanitising pasted text

`ratatui`'s `Span` renderer filters only `\n` and writes any other control
character straight into a cell, so a clipboard payload carrying a raw `ESC` (or
`BEL`, C1, …) could drive the terminal after a paste. `PastePolicy::SingleLine`
therefore routes every paste through `strip_control_chars`, which drops **every**
control character; `PastePolicy::Digits` keeps only ASCII digits. Typed input is
guarded the same way before any insert: `theming::Action::insert` rejects a
control character, and the File Browser's text-edit and search handlers apply
the same rule.

This is a deliberate tightening: a control character can no longer be typed or
pasted into a field, so a value that previously carried one (a tab pasted into a
passphrase) must be re-entered.

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
