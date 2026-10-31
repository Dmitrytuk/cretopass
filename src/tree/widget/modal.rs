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
mod export;
mod help;

pub(crate) use alert::Alert;
pub(crate) use detail::Detail;
pub(crate) use export::ExportWizard;
pub(crate) use help::Help;

pub(crate) use chrome::centered_rect;

/// Items the tree's tests reach for; the production build uses them only inside
/// this module tree.
#[cfg(test)]
pub(crate) use detail::{
    GENERATOR_ROW_GENERATE, GENERATOR_ROW_LENGTH, GENERATOR_ROW_SYMBOLS, PickerItem, form_hint,
    picker_items, render_extra_field_line,
};

use crossterm::event::KeyEvent;

use super::TreeState;
use crate::file_handling::export::ExportFormat;
use crate::tree::node::Node;

/// What the active modal asks the Tree screen to do.
///
/// Variants are added by the commit that migrates the flow producing them, so the
/// enum never carries a case nothing constructs.
#[derive(Debug)]
pub(crate) enum TreeAction {
    /// Dismiss the modal and reveal what is underneath.
    Close,
    /// Write `text` to the system clipboard.
    CopyToClipboard { text: String },
    /// Write `node` (with or without its children) to a plain-text file beside
    /// the secret file, in `format`.
    Export {
        node: Node,
        include_children: bool,
        format: ExportFormat,
    },
}

/// The Tree's modal slot: at most one at a time, over Normal navigation.
#[derive(Debug)]
pub(crate) enum Modal {
    /// The key-binding list, opened with `?`.
    Help(Help),
    /// A passive message the user acknowledges with any key.
    Alert(Alert),
    /// A selected Node's extra fields, with copy and edit. Boxed because it is
    /// far larger than the passive modals.
    Detail(Box<Detail>),
    /// The two-step export flow: pick a scope, then a format.
    Export(ExportWizard),
}

impl Modal {
    /// A passive alert carrying `message`.
    pub(crate) fn alert(message: impl Into<String>) -> Self {
        Self::Alert(Alert {
            message: message.into(),
        })
    }

    /// Reads a key: `None` means the modal consumed it and stays open, `Some` is
    /// what the user asked for.
    ///
    /// `state` is the tree the modal may read and mutate in memory; all I/O is
    /// still deferred to the action.
    pub(crate) fn handle_key(
        &mut self,
        key: KeyEvent,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        match self {
            Self::Help(help) => help.handle_key(key, state),
            Self::Alert(alert) => alert.handle_key(key, state),
            Self::Detail(detail) => detail.handle_key(key, state),
            Self::Export(wizard) => wizard.handle_key(key, state),
        }
    }
}
