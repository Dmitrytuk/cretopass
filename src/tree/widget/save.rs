//! The Tree's autosave status and the parked exit it can produce.

use crate::file_handling::SaveError;
use crate::theming::{Status, texts};

/// Persistence status of the Tree's background autosave.
///
/// Replaces the previous `status` / `save_error` pair, which could disagree. A
/// failed save keeps its [`SaveError`] until the next attempt, and tracks
/// whether the user has dismissed the error overlay.
#[derive(Debug)]
pub enum SaveState {
    /// No save is outstanding; the last attempt (if any) succeeded.
    Idle,
    /// The latest submitted snapshot is being written.
    Saving,
    /// The latest save attempt failed.
    Failed {
        /// The error reported by the save function.
        error: SaveError,
        /// Whether the error overlay has been dismissed.
        acknowledged: bool,
    },
}

impl SaveState {
    /// Title-bar status for this state, or `None` when there is nothing to show.
    pub fn title_status(&self) -> Option<Status> {
        match self {
            Self::Idle => None,
            Self::Saving => Some(Status::Saving),
            Self::Failed { .. } => Some(Status::SaveFailed),
        }
    }

    /// Whether the title-bar status is a busy state that should spin.
    pub fn spins(&self) -> bool {
        matches!(self, Self::Saving)
    }

    /// Message for the save-error overlay, or `None` when it should not show.
    ///
    /// Returns `Some` only while a failure is unacknowledged; the first key
    /// dismisses it via [`SaveState::acknowledge`].
    pub fn overlay_message(&self) -> Option<String> {
        match self {
            Self::Failed {
                error,
                acknowledged: false,
            } => Some(format!("{}{}", texts::MSG_SAVE_FAILED, error)),
            Self::Idle | Self::Saving | Self::Failed { .. } => None,
        }
    }

    /// Marks a failure as acknowledged so its overlay stops showing.
    pub fn acknowledge(&mut self) {
        if let Self::Failed { acknowledged, .. } = self {
            *acknowledged = true;
        }
    }

    /// Whether the latest save attempt failed.
    pub fn has_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// State of a requested exit that is waiting on the autosave worker.
///
/// A `q` / `H` / `Ctrl-C` request whose final flush fails parks here so the
/// user can retry, quit without saving, or cancel — instead of being trapped in
/// the Tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveExit {
    /// No exit is parked; the flush branch may run.
    None,
    /// The exit flush failed; the overlay offers retry / quit-anyway / stay.
    Blocked,
    /// The user chose to quit without saving; awaiting confirmation.
    ConfirmDiscard,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_state_idle_has_no_title_status() {
        assert_eq!(SaveState::Idle.title_status(), None);
        assert!(!SaveState::Idle.spins());
    }

    #[test]
    fn save_state_saving_spins() {
        assert_eq!(SaveState::Saving.title_status(), Some(Status::Saving));
        assert!(SaveState::Saving.spins());
    }

    #[test]
    fn save_state_failed_is_static() {
        let failed = SaveState::Failed {
            error: SaveError::WorkerStopped,
            acknowledged: false,
        };
        assert_eq!(failed.title_status(), Some(Status::SaveFailed));
        assert!(!failed.spins());
    }
}
