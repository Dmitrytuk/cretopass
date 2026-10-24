//! The key-handling contract shared by the Tree's modals.
//!
//! A modal reports what the user asked for as a [`TreeAction`]. It never touches
//! the filesystem, never draws, and never builds another modal — the orchestrator
//! performs the action. A modal that reads and writes the tree (like Detail,
//! whose Form Popup live-edits a Node) receives the [`TreeState`] to do that
//! in-memory work, but still reports its I/O (clipboard, export) as actions.
//!
//! This is the seam that lets the Tree's flows be tested without a terminal.

mod alert;
mod chrome;
mod detail;
mod help;

pub use alert::Alert;
pub use chrome::make_overlay_area;
pub use detail::Detail;
pub use help::Help;

use crossterm::event::KeyEvent;

use super::TreeState;

/// What the active modal asks the Tree screen to do.
///
/// Variants are added by the commit that migrates the flow producing them, so the
/// enum never carries a case nothing constructs.
#[derive(Debug)]
pub enum TreeAction {
    /// Dismiss the modal and reveal what is underneath.
    Close,
    /// Write `text` to the system clipboard.
    CopyToClipboard { text: String },
    /// Open the Form Popup for the selected Node.
    OpenEdit,
}

/// The Tree's modal slot: at most one at a time, over Normal navigation.
#[derive(Debug)]
pub enum Modal {
    /// The key-binding list, opened with `?`.
    Help(Help),
    /// A passive message the user acknowledges with any key.
    Alert(Alert),
    /// A selected Node's extra fields, with copy and edit.
    Detail(Detail),
}

impl Modal {
    /// Reads a key: `None` means the modal consumed it and stays open, `Some` is
    /// what the user asked for.
    ///
    /// `state` is the tree the modal may read and mutate in memory; all I/O is
    /// still deferred to the action.
    pub fn handle_key(&mut self, key: KeyEvent, state: &mut TreeState) -> Option<TreeAction> {
        match self {
            Self::Help(help) => help.handle_key(key, state),
            Self::Alert(alert) => alert.handle_key(key, state),
            Self::Detail(detail) => detail.handle_key(key, state),
        }
    }
}
