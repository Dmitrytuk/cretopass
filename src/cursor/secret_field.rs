//! Masked password fields shared by the Home prompts and the Share flow.
//!
//! The unlock, create-secret and change-password prompts all edit N masked text
//! fields with one focused field and a shared "reveal plaintext" flag. This
//! module owns that behaviour on top of the [`super::core`] editing primitives,
//! so every prompt stops re-implementing the same editing and span-building
//! code.

use ratatui::text::Span;
use zeroize::Zeroize;

use super::core::{backspace_at, cursor_left, cursor_right, delete_at, insert_char_at};
use crate::theming::ElementStyle;

/// A single masked text field: its text and a byte-offset cursor
/// ([ADR 0003](docs/adr/0003-cursor.md)).
///
/// The text is a plaintext secret: it is wiped from the heap when the field is
/// dropped ([`Zeroize`]) and redacted from [`Debug`], mirroring `SaveTarget`.
#[derive(Default, Clone)]
pub struct SecretField {
    text: String,
    cursor: usize,
}

impl std::fmt::Debug for SecretField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretField")
            .field("text", &"<redacted>")
            .field("cursor", &self.cursor)
            .finish()
    }
}

impl Drop for SecretField {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

impl SecretField {
    /// The field's text, always masked at render time unless revealed.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The cursor position as a byte offset into [`SecretField::text`].
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn insert(&mut self, c: char) {
        insert_char_at(&mut self.text, &mut self.cursor, c);
    }

    pub fn backspace(&mut self) {
        self.cursor = backspace_at(&mut self.text, self.cursor);
    }

    pub fn delete(&mut self) {
        delete_at(&mut self.text, self.cursor);
    }

    pub fn move_left(&mut self) {
        self.cursor = cursor_left(&self.text, self.cursor);
    }

    pub fn move_right(&mut self) {
        self.cursor = cursor_right(&self.text, self.cursor);
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }

    fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }
}

/// A group of [`SecretField`]s with one focused field and one reveal flag.
///
/// The prompts differ only in how many fields they hold: one for unlocking, two
/// for creating a secret file or a share bundle, three for changing the master
/// password.
#[derive(Default, Clone)]
pub struct SecretFieldGroup {
    fields: Vec<SecretField>,
    focused: usize,
    show: bool,
}

impl std::fmt::Debug for SecretFieldGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Each field redacts its own text; print the shape, not the contents.
        f.debug_struct("SecretFieldGroup")
            .field("fields", &self.fields.len())
            .field("focused", &self.focused)
            .field("show", &self.show)
            .finish()
    }
}

impl SecretFieldGroup {
    /// A group of `count` empty fields, the first one focused, text hidden.
    pub fn with_fields(count: usize) -> Self {
        Self {
            fields: vec![SecretField::default(); count],
            focused: 0,
            show: false,
        }
    }

    pub fn field(&self, index: usize) -> &SecretField {
        &self.fields[index]
    }

    pub fn focused_index(&self) -> usize {
        self.focused
    }

    pub fn show(&self) -> bool {
        self.show
    }

    pub fn toggle_show(&mut self) {
        self.show = !self.show;
    }

    /// Focuses the next field, wrapping to the first one.
    pub fn next_field(&mut self) {
        let count = self.fields.len();
        if count > 0 {
            self.focused = (self.focused + 1) % count;
        }
    }

    /// Focuses the previous field, stopping at the first one.
    pub fn prev_field(&mut self) {
        self.focused = self.focused.saturating_sub(1);
    }

    /// Runs `action` on the focused field.
    pub fn apply(&mut self, action: impl FnOnce(&mut SecretField)) {
        if let Some(field) = self.fields.get_mut(self.focused) {
            action(field);
        }
    }

    /// Moves every cursor to the start and focuses the first field, keeping the
    /// typed text and the reveal flag (the "fix your input" path).
    pub fn reset_cursors(&mut self) {
        for field in &mut self.fields {
            field.move_home();
        }
        self.focused = 0;
    }

    /// Empties every field, focuses the first one and hides the text (the "start
    /// over" path).
    pub fn clear(&mut self) {
        for field in &mut self.fields {
            field.clear();
        }
        self.focused = 0;
        self.show = false;
    }
}

/// Spans for the value of `field`: masked unless `show`, with an inverted cursor
/// cell while the field is focused.
pub fn secret_field_spans(field: &SecretField, focused: bool, show: bool) -> Vec<Span<'static>> {
    let display = if show {
        field.text().to_string()
    } else {
        "*".repeat(field.text().len())
    };

    if !focused {
        return vec![Span::styled(display, ElementStyle::HomeFieldLabel.style())];
    }

    let pos = field.cursor().min(display.len());
    let (before, rest) = display.split_at(pos);
    let ch = rest.chars().next();
    let after = &rest[ch.map(|c| c.len_utf8()).unwrap_or(0)..];

    let mut spans = Vec::new();
    if !before.is_empty() {
        spans.push(Span::raw(before.to_string()));
    }
    spans.push(Span::styled(
        ch.map(|c| c.to_string()).unwrap_or_else(|| " ".to_string()),
        ElementStyle::InvertedCursor.style(),
    ));
    if !after.is_empty() {
        spans.push(Span::raw(after.to_string()));
    }
    spans
}

/// The padded `  Label: ` prefix followed by [`secret_field_spans`]: the layout
/// the multi-field prompts use.
pub fn password_field_spans(
    label: &str,
    field: &SecretField,
    focused: bool,
    show: bool,
) -> Vec<Span<'static>> {
    let mut spans = vec![Span::styled(
        format!("  {label}: "),
        ElementStyle::HomeFieldLabel.style(),
    )];
    spans.extend(secret_field_spans(field, focused, show));
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(chars: &str) -> SecretField {
        let mut field = SecretField::default();
        for c in chars.chars() {
            field.insert(c);
        }
        field
    }

    #[test]
    fn editing_keeps_the_cursor_on_a_char_boundary() {
        let mut field = typed("aй");
        assert_eq!(field.text(), "aй");
        assert_eq!(field.cursor(), 3);

        field.move_left();
        assert_eq!(field.cursor(), 1);
        field.backspace();
        assert_eq!(field.text(), "й");
        assert_eq!(field.cursor(), 0);

        field.delete();
        assert_eq!(field.text(), "");
    }

    #[test]
    fn home_and_end_move_to_the_edges() {
        let mut field = typed("secret");
        field.move_home();
        assert_eq!(field.cursor(), 0);
        field.move_end();
        assert_eq!(field.cursor(), "secret".len());
    }

    #[test]
    fn apply_targets_only_the_focused_field() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.apply(|f| f.insert('a'));
        group.next_field();
        group.apply(|f| f.insert('b'));

        assert_eq!(group.field(0).text(), "a");
        assert_eq!(group.field(1).text(), "b");
    }

    #[test]
    fn focus_wraps_forward_and_stops_backward() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.prev_field();
        assert_eq!(group.focused_index(), 0);

        group.next_field();
        assert_eq!(group.focused_index(), 1);
        group.next_field();
        assert_eq!(group.focused_index(), 0);
    }

    #[test]
    fn reset_cursors_keeps_the_text_but_clear_does_not() {
        let mut group = SecretFieldGroup::with_fields(2);
        group.apply(|f| f.insert('x'));
        group.next_field();
        group.apply(|f| f.insert('y'));
        group.toggle_show();

        group.reset_cursors();
        assert_eq!(group.focused_index(), 0);
        assert_eq!(group.field(0).text(), "x");
        assert_eq!(group.field(1).text(), "y");
        assert_eq!(group.field(1).cursor(), 0);
        assert!(group.show(), "reset_cursors must keep the reveal flag");

        group.clear();
        assert_eq!(group.field(0).text(), "");
        assert_eq!(group.field(1).text(), "");
        assert!(!group.show(), "clear must hide the text again");
    }

    #[test]
    fn masked_value_hides_every_byte_for_multibyte_text() {
        let field = typed("й");
        let spans = secret_field_spans(&field, false, false);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "**");

        let revealed = secret_field_spans(&field, false, true);
        assert_eq!(revealed[0].content, "й");
    }

    #[test]
    fn focused_value_wraps_the_cursor_cell() {
        let mut field = typed("abc");
        field.move_left();
        field.move_left(); // cursor sits between `a` and `b`

        let spans = secret_field_spans(&field, true, true);
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "abc");
        assert_eq!(spans.len(), 3, "before, cursor cell, after");
        assert_eq!(
            spans[1].content, "b",
            "the cell under the cursor is the next char"
        );
        assert_eq!(spans[1].style, ElementStyle::InvertedCursor.style());
    }
}
