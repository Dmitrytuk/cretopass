/// Every glyph the UI draws as an icon.
///
/// Glyphs must be within WGL4 (or plain ASCII), because the console fonts
/// Windows ships are WGL4-level: a codepoint outside it — such as `U+25B6`
/// `▶` / `U+25C0` `◀`, or the spinner's Braille range — has no glyph in the
/// font and Windows substitutes it from Segoe UI Symbol/Emoji, drawing the
/// boxed emoji form instead. The expand/collapse pair is therefore the
/// WGL4-safe `►` / `▼`.
#[derive(Debug, Clone, Copy)]
pub enum Icon {
    SelectionArrow,
    SelectionPad,
    Expanded,
    Collapsed,
    LeafPad,
    PwdMarker,
    CheckboxOn,
    CheckboxOff,
}

impl Icon {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SelectionArrow => "▸ ",
            Self::SelectionPad => "  ",
            Self::Expanded => "▼ ",
            Self::Collapsed => "► ",
            Self::LeafPad => "  ",
            Self::PwdMarker => "[PWD] ",
            Self::CheckboxOn => "[X]",
            Self::CheckboxOff => "[ ]",
        }
    }
}
