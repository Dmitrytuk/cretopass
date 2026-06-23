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
            Self::Collapsed => "▶ ",
            Self::LeafPad => "  ",
            Self::PwdMarker => "[PWD] ",
            Self::CheckboxOn => "[X]",
            Self::CheckboxOff => "[ ]",
        }
    }
}
