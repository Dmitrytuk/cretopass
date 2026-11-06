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
mod export_destination;
mod help;
mod share;
mod share_destination;
mod share_passphrase;

pub(crate) use alert::Alert;
pub(crate) use detail::Detail;
pub(crate) use export::ExportWizard;
pub(crate) use export_destination::ExportDestination;
pub(crate) use help::Help;
pub(crate) use share::ShareWizard;
pub(crate) use share_destination::ShareDestination;
pub(crate) use share_passphrase::SharePassphrase;

pub(crate) use chrome::centered_rect;

/// Items the tree's tests reach for; the production build uses them only inside
/// this module tree.
#[cfg(test)]
pub(crate) use detail::{
    GENERATOR_ROW_GENERATE, GENERATOR_ROW_LENGTH, GENERATOR_ROW_SYMBOLS, PickerItem, form_hint,
    picker_items, render_extra_field_line,
};

use crossterm::event::{KeyCode, KeyEvent, MouseButton};
use ratatui::layout::Rect;

use super::TreeState;
use crate::file_handling::ExportFormat;
use crate::hitbox::{is_left, point_in};
use crate::share::SharePassword;
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
    /// Open the file browser to name the destination of an export of `node`,
    /// with or without its children, in `format`.
    BeginExportDestination {
        node: Node,
        include_children: bool,
        format: ExportFormat,
    },
    /// Write `node` (with or without its children) to `path` as plain text.
    Export {
        node: Node,
        include_children: bool,
        format: ExportFormat,
        path: String,
    },
    /// Open the File Browser to name the folder a share bundle of `node`, with
    /// or without its children, is written into.
    BeginShareDestination { node: Node, include_children: bool },
    /// Ask for the password a share bundle of `node` is encrypted with, having
    /// named `dest`.
    BeginSharePassphrase {
        node: Node,
        include_children: bool,
        dest: String,
    },
    /// Write a share bundle of `node` into `dest`, encrypted with `passphrase`.
    Share {
        node: Node,
        include_children: bool,
        dest: String,
        passphrase: SharePassword,
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
    /// Naming the destination of an export in the File Browser. Boxed because the
    /// browser's state is far larger than the passive modals.
    ExportDestination(Box<ExportDestination>),
    /// The share flow's first step: pick what to share (this node or with its
    /// children).
    Share(ShareWizard),
    /// Naming the folder a share bundle is written into. Boxed because the
    /// browser's state is far larger than the passive modals.
    ShareDestination(Box<ShareDestination>),
    /// The share flow's password step. Boxed for the same reason.
    SharePassphrase(Box<SharePassphrase>),
}

impl Modal {
    /// A passive alert reporting a successful outcome.
    pub(crate) fn success(message: impl Into<String>) -> Self {
        Self::Alert(Alert::success(message))
    }

    /// A passive alert reporting a refusal or a failure.
    pub(crate) fn error(message: impl Into<String>) -> Self {
        Self::Alert(Alert::error(message))
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
            Self::ExportDestination(destination) => destination.handle_key(key, state),
            Self::Share(wizard) => wizard.handle_key(key, state),
            Self::ShareDestination(destination) => destination.handle_key(key, state),
            Self::SharePassphrase(prompt) => prompt.handle_key(key, state),
        }
    }

    /// Applies pasted `text` to the modal's active text surface, when it has one.
    ///
    /// Each modal owns whether it accepts a paste and how it is sanitised; the
    /// passive Help/Alert modals and the two scope/format wizards change nothing.
    pub(crate) fn handle_paste(&mut self, text: &str, state: &mut TreeState) {
        match self {
            Self::Detail(detail) => detail.handle_paste(text, state),
            Self::ExportDestination(destination) => destination.handle_paste(text),
            Self::ShareDestination(destination) => destination.handle_paste(text),
            Self::SharePassphrase(prompt) => prompt.handle_paste(text),
            Self::Help(_) | Self::Alert(_) | Self::Export(_) | Self::Share(_) => {}
        }
    }

    /// Applies a click at `(x, y)`: the modal hit-tests it against the regions its
    /// renderer recorded and reports what the user asked for, exactly as
    /// [`Modal::handle_key`] does.
    ///
    /// A left click **outside** the panel the renderer drew is the modal's `Esc`:
    /// the existing key path runs, so the click and the key can never drift. That
    /// closes a form, steps a wizard back, unwinds one nested popup, or declines a
    /// confirmation — whatever `Esc` does on the active screen. Help and Alert
    /// dismiss anywhere, so they report no panel; the File Browser-backed
    /// ExportDestination and ShareDestination steps are a deliberate exception,
    /// for the reason given on [`Modal::panel_rect`].
    pub(crate) fn handle_click(
        &mut self,
        button: MouseButton,
        x: u16,
        y: u16,
        state: &mut TreeState,
    ) -> Option<TreeAction> {
        if is_left(button)
            && let Some(rect) = self.panel_rect()
            && !point_in(rect, x, y)
        {
            return self.handle_key(KeyEvent::from(KeyCode::Esc), state);
        }
        match self {
            Self::Help(help) => help.handle_click(button, x, y, state),
            Self::Alert(alert) => alert.handle_click(button, x, y, state),
            Self::Detail(detail) => detail.handle_click(button, x, y, state),
            Self::Export(wizard) => wizard.handle_click(button, x, y, state),
            Self::ExportDestination(destination) => destination.handle_click(button, x, y, state),
            Self::Share(wizard) => wizard.handle_click(button, x, y, state),
            Self::ShareDestination(destination) => destination.handle_click(button, x, y, state),
            Self::SharePassphrase(prompt) => prompt.handle_click(button, x, y, state),
        }
    }

    /// The panel the active modal drew last frame, when a click outside it is a
    /// dismissal.
    ///
    /// `None` for the overlays that already dismiss on any click (Help, Alert) and
    /// for the File Browser-backed destination steps. The destination steps are a
    /// deliberate exception: their clickable list fills most of the screen, so a
    /// click beside it is far more likely to be a near-miss on a row than a
    /// request to abandon the whole export/share flow — they keep their own click
    /// handling instead.
    pub(crate) fn panel_rect(&self) -> Option<Rect> {
        match self {
            Self::Detail(detail) => detail.panel_rect(),
            Self::Export(wizard) => wizard.panel_rect(),
            Self::Share(wizard) => wizard.panel_rect(),
            Self::SharePassphrase(prompt) => prompt.panel_rect(),
            Self::Help(_)
            | Self::Alert(_)
            | Self::ExportDestination(_)
            | Self::ShareDestination(_) => None,
        }
    }
}
