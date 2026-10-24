//! The key-handling contract shared by the Tree's modals.
//!
//! A modal on this contract is pure input: [`Modal::handle_key`] reads a key and
//! reports what the user asked for as a [`TreeAction`]. It never touches the
//! filesystem, never draws, and never builds another modal — the orchestrator
//! performs the action. Everything else in a modal's behaviour (validation,
//! focus, masking) stays inside the modal with its own unit tests.
//!
//! This is the seam that lets the Tree's flows be tested without a terminal.

mod alert;
mod help;

pub use alert::Alert;
pub use help::Help;

use crossterm::event::KeyEvent;

/// What the active modal asks the Tree screen to do.
///
/// Variants are added by the commit that migrates the flow producing them, so the
/// enum never carries a case nothing constructs.
#[derive(Debug)]
pub enum TreeAction {
    /// Dismiss the modal and reveal what is underneath.
    Close,
}

/// The Tree's modal slot: at most one at a time, over Normal navigation.
#[derive(Debug)]
pub enum Modal {
    /// The key-binding list, opened with `?`.
    Help(Help),
    /// A passive message the user acknowledges with any key.
    Alert(Alert),
}

impl Modal {
    /// Reads a key: `None` means the modal consumed it and stays open, `Some` is
    /// what the user asked for.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<TreeAction> {
        match self {
            Self::Help(help) => help.handle_key(key),
            Self::Alert(alert) => alert.handle_key(key),
        }
    }
}
