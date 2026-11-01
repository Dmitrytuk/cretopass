//! The text-editing key map shared by the Browser's name and rename inputs.

use crossterm::event::KeyCode;

/// Handles common text-editing key events (Char, Backspace, Delete, Left,
/// Right, Home, End).  Returns `true` if the key was consumed by editing.
pub(super) fn handle_text_edit(cursor: &mut usize, text: &mut String, key: KeyCode) -> bool {
    match key {
        KeyCode::Char(c) => {
            crate::cursor::insert_char_at(text, cursor, c);
            true
        }
        KeyCode::Backspace => {
            if *cursor > 0 {
                *cursor = crate::cursor::backspace_at(text, *cursor);
            }
            true
        }
        KeyCode::Delete => {
            crate::cursor::delete_at(text, *cursor);
            true
        }
        KeyCode::Left => {
            *cursor = crate::cursor::cursor_left(text, *cursor);
            true
        }
        KeyCode::Right => {
            *cursor = crate::cursor::cursor_right(text, *cursor);
            true
        }
        KeyCode::Home => {
            *cursor = 0;
            true
        }
        KeyCode::End => {
            *cursor = text.len();
            true
        }
        _ => false,
    }
}
