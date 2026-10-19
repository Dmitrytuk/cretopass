//! Busy/failure status shown in the Home and Tree title bars.
//!
//! [`Status`] is the single vocabulary both screens use to label a title-bar
//! status. Busy states ([`Status::Loading`], [`Status::Saving`]) spin; a failure
//! ([`Status::SaveFailed`]) is a static label.

use crate::theming::{ElementStyle, texts};

/// A title-bar status message that may animate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// A blocking load (decrypt / open) is running.
    Loading,
    /// A save is running.
    Saving,
    /// The latest save attempt failed.
    SaveFailed,
}

impl Status {
    /// Text label shown after the (optional) spinning glyph.
    pub fn label(self) -> &'static str {
        match self {
            Self::Loading => texts::STATUS_LOADING,
            Self::Saving => texts::STATUS_SAVING,
            Self::SaveFailed => texts::STATUS_SAVE_FAILED,
        }
    }

    /// Whether this status is busy and should show an animated glyph.
    pub fn spins(self) -> bool {
        matches!(self, Self::Loading | Self::Saving)
    }

    /// Title-bar style for this status.
    pub fn style(self) -> ElementStyle {
        match self {
            Self::Loading => ElementStyle::StatusLoading,
            Self::Saving => ElementStyle::StatusSaving,
            Self::SaveFailed => ElementStyle::StatusSaveFailed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_busy_statuses_spin() {
        assert!(Status::Loading.spins());
        assert!(Status::Saving.spins());
        assert!(!Status::SaveFailed.spins());
    }

    #[test]
    fn labels_match_the_text_constants() {
        assert_eq!(Status::Loading.label(), texts::STATUS_LOADING);
        assert_eq!(Status::Saving.label(), texts::STATUS_SAVING);
        assert_eq!(Status::SaveFailed.label(), texts::STATUS_SAVE_FAILED);
    }

    #[test]
    fn styles_map_per_status() {
        assert!(matches!(
            Status::Loading.style(),
            ElementStyle::StatusLoading
        ));
        assert!(matches!(Status::Saving.style(), ElementStyle::StatusSaving));
        assert!(matches!(
            Status::SaveFailed.style(),
            ElementStyle::StatusSaveFailed
        ));
    }
}
