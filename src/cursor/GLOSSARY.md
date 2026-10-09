# Cursor

UTF-8-aware text editing primitives operating on byte offsets and TUI cursor field rendering.

## Language

**Cursor**: Position within text, measured as a byte offset from the start of the string. Used by all editing operations (`cursor_left`, `cursor_right`, `insert_char_at`, `backspace_at`, `delete_at`).
_Avoid_: Position, Caret, Pointer

**Byte offset**: A byte index from the start of the string, always landing on a UTF-8 character boundary. Cursor movement operations guarantee the cursor never sits inside a multi-byte character.
_Avoid_: Position, Index, Character index

**Cursor field**: An editable text area rendered via `cursor_field_spans()`. Consists of a label, text content, and a visible inverted cursor at the editing position. Can be active (cursor visible, normal text style) or inactive (dimmed text, cursor hidden).
_Avoid_: Text input, Editable field, Text box

**Inverted cursor**: The rendering style for the character under cursor in an active Cursor field: white background, black text. If the cursor is past the end of the text, a space with the same style is displayed instead.
_Avoid_: Cursor highlight, Caret, Blinking block