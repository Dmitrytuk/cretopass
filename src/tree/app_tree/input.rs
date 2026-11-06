//! Input routing: keys, mouse and paste, dispatched to the tree or the modal.
//!
//! Every input device funnels through the shared classifier
//! [`route_key`](crate::theming::route_key) and [`handle_mouse`] so a click and
//! its key run the same code: the renderer owns the geometry, a click maps to the
//! same [`Action`](crate::theming::Action) a key would produce, and
//! [`dispatch_action`] performs it. A modal is taken out of the state for the
//! duration of its handler so it can borrow the tree.

use std::io;

use crate::clipboard;
use crate::cursor::PastePolicy;
use crate::file_handling::{Autosave, SaveError};
use crate::theming::{Action, KeyRoute, route_key, wheel_nav_key};
use crate::tree::node::Node;
use crate::tree::widget::{
    AddPosition, Detail, ExportWizard, Help, InputMode, Modal, SaveExit, SaveState, ShareWizard,
    TreeAction, TreeHit, TreeState, execute,
};
use crossterm::event::{self, MouseButton, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::Backend;
use zeroize::Zeroizing;

use super::r#loop::{AutosaveAction, autosave_action};
use super::render::ui;

/// Runs one key press through routing and the autosave bookkeeping.
///
/// Both a real key and a wheel tick arrive here, so the two input devices cannot
/// diverge: a wheel tick is only an `Up`/`Down` key (see [`wheel_nav_key`]).
pub(super) fn handle_key_press<B: Backend>(
    key: event::KeyEvent,
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
    autosave: &mut Option<Autosave<Node>>,
    submitted: &mut u64,
) -> io::Result<()> {
    match route_key(&key, state.modal.is_some()) {
        KeyRoute::Paste => paste_from_clipboard(state),
        KeyRoute::Modal => handle_modal_key(key, state, terminal)?,
        KeyRoute::Screen => dispatch_key(key, state),
    }
    flush_autosave(state, autosave, submitted);
    Ok(())
}

/// Hands a committed change to the autosave worker, showing `Saving` until it
/// has been submitted.
fn flush_autosave(
    state: &mut TreeState,
    autosave: &mut Option<Autosave<Node>>,
    submitted: &mut u64,
) {
    let Some(worker) = autosave else {
        return;
    };
    match autosave_action(state.modified, worker.is_running(), &state.save_state) {
        AutosaveAction::Submit => {
            state.modified = false;
            *submitted = worker.submit(state.root.clone());
            state.save_state = SaveState::Saving;
        }
        AutosaveAction::ReportWorkerStopped => {
            state.save_state = SaveState::Failed {
                error: SaveError::WorkerStopped,
                acknowledged: false,
            };
        }
        AutosaveAction::Idle => {}
    }
}

/// Applies a mouse event.
///
/// A wheel tick scrolls the active list through the same key path as the arrows;
/// a left/right button press is hit-tested against the active modal or the tree
/// rows. The save overlay owns the screen — its keys answer a prompt, they do
/// not navigate — so every mouse event is ignored while it is up. The wheel is
/// additionally skipped on a dismiss overlay (Help or an Alert), because it must
/// never be the key that closes it; a click there *does* dismiss it.
pub(super) fn handle_mouse<B: Backend>(
    mouse: event::MouseEvent,
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
    autosave: &mut Option<Autosave<Node>>,
    submitted: &mut u64,
) -> io::Result<()> {
    if state.save_exit != SaveExit::None || state.save_state.overlay_message().is_some() {
        return Ok(());
    }
    match mouse.kind {
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
            if dismiss_overlay_open(state) {
                return Ok(());
            }
            if let Some(key) = wheel_nav_key(&mouse) {
                handle_key_press(key, state, terminal, autosave, submitted)?;
            }
        }
        MouseEventKind::Down(button) => {
            handle_click(
                button,
                mouse.column,
                mouse.row,
                state,
                terminal,
                autosave,
                submitted,
            )?;
        }
        _ => {}
    }
    Ok(())
}

/// Turns a left/right button press into the action the click stands for.
///
/// The active modal owns the screen: a click goes to it, or does nothing. With
/// no modal, the click is hit-tested against the tree rows; `Esc`-only modes
/// (search and the prefixes) accept no clicks.
pub(super) fn handle_click<B: Backend>(
    button: MouseButton,
    x: u16,
    y: u16,
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
    autosave: &mut Option<Autosave<Node>>,
    submitted: &mut u64,
) -> io::Result<()> {
    if state.modal.is_some() {
        handle_modal_click(button, x, y, state, terminal)?;
    } else if matches!(state.mode, InputMode::Normal)
        && let Some(target) = state.hits.hit(x, y, button)
    {
        apply_tree_hit(target, button, state);
    }
    flush_autosave(state, autosave, submitted);
    Ok(())
}

/// Applies a click on a tree row.
///
/// The click always moves the selection to the clicked row first (the rule the
/// whole mouse feature follows): a left click on a branch then toggles it, a
/// left click on a leaf only selects it, and a right click opens the row's
/// Detail — the same view `i` opens.
fn apply_tree_hit(target: TreeHit, button: MouseButton, state: &mut TreeState) {
    let TreeHit::Row(index) = target;
    if index >= state.visible.len() {
        return;
    }
    state.selected = index;
    match button {
        // A branch toggles through the same action its `Enter`/`Space` key uses.
        MouseButton::Left if state.visible[index].is_parent => {
            dispatch_action(Action::ToggleExpand, state)
        }
        MouseButton::Right => state.modal = Some(Modal::Detail(Box::new(Detail::browse()))),
        _ => {}
    }
}

/// Dispatches a click to the active modal, mirroring [`handle_modal_key`].
fn handle_modal_click<B: Backend>(
    button: MouseButton,
    x: u16,
    y: u16,
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let Some(mut modal) = state.modal.take() else {
        return Ok(());
    };
    match modal.handle_click(button, x, y, state) {
        None => {
            state.modal = Some(modal);
            Ok(())
        }
        Some(action) => run_modal_action(state, terminal, modal, action),
    }
}

/// Whether a modal that closes on any key is open. The wheel must not be that
/// key.
fn dismiss_overlay_open(state: &TreeState) -> bool {
    matches!(state.modal, Some(Modal::Help(_) | Modal::Alert(_)))
}

/// Dispatches a key event to the handler for the current input mode.
pub(super) fn dispatch_key(key: event::KeyEvent, state: &mut TreeState) {
    match &state.mode {
        InputMode::Normal => dispatch_action(Action::from_normal(&key), state),
        InputMode::Search => handle_search(key, state),
        InputMode::PendingCut => handle_pending_cut(key, state),
        InputMode::PendingPaste => handle_pending_paste(key, state),
        InputMode::PendingAdd => handle_pending_add(key, state),
        InputMode::PendingExport => handle_pending_export(key, state),
    }
}

/// Dispatches a key to the active modal.
///
/// The modal reads the key itself and reports a [`TreeAction`], which [`execute`]
/// then performs. The modal is taken out of [`TreeState`] for the duration, so an
/// action that dismisses it simply does not put it back.
pub(super) fn handle_modal_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let Some(mut modal) = state.modal.take() else {
        return Ok(());
    };

    match modal.handle_key(key, state) {
        // The modal consumed the key and stays open.
        None => {
            state.modal = Some(modal);
            Ok(())
        }
        Some(action) => run_modal_action(state, terminal, modal, action),
    }
}

/// Performs a modal action, identically for a key press and a click.
///
/// The File Browser remembers where it was for its next opening, the same way
/// Home captures it when the browser reports an action. A `Close` leaves the slot
/// as `execute` left it; any other action keeps the modal that produced it,
/// unless `execute` installed a replacement (Export, Share and a copy all report
/// their result as an Alert).
fn run_modal_action<B: Backend>(
    state: &mut TreeState,
    terminal: &mut Terminal<B>,
    modal: Modal,
    action: TreeAction,
) -> io::Result<()> {
    match &modal {
        Modal::ExportDestination(destination) => {
            state.browser_session = destination.session();
        }
        Modal::ShareDestination(destination) => {
            state.browser_session = destination.session();
        }
        _ => {}
    }
    let keeps_produced = !matches!(action, TreeAction::Close);
    let result = execute(state, terminal, action, ui);
    state.modal = retained_modal(keeps_produced, modal, state.modal.take());
    result
}

/// The modal showing after a modal action runs.
///
/// A `Close` action leaves the slot as `execute` left it (normally empty). Any
/// other action keeps the modal that produced it, unless `execute` installed a
/// replacement (Export, Share and a copy all report their result as an Alert).
/// Keeping the produced modal matters on the busy-runner error path:
/// `export`/`share` run `run_with_busy(...)?` before installing their result, so
/// when that `?` returns early the slot is empty and this fallback is what keeps
/// the produced modal on screen.
pub(super) fn retained_modal(
    keeps_produced: bool,
    produced: Modal,
    installed: Option<Modal>,
) -> Option<Modal> {
    if keeps_produced {
        installed.or(Some(produced))
    } else {
        installed
    }
}

/// Whether the open export/share destination browser has a listing in flight.
pub(super) fn browser_loading(state: &TreeState) -> bool {
    match &state.modal {
        Some(Modal::ExportDestination(destination)) => destination.is_loading(),
        Some(Modal::ShareDestination(destination)) => destination.is_loading(),
        _ => false,
    }
}

/// Applies a completed destination-browser listing, if any.
pub(super) fn poll_browser_listing(state: &mut TreeState) {
    match &mut state.modal {
        Some(Modal::ExportDestination(destination)) => {
            destination.poll_listing();
        }
        Some(Modal::ShareDestination(destination)) => {
            destination.poll_listing();
        }
        _ => {}
    }
}

fn is_leaf_selected(state: &TreeState) -> bool {
    state
        .visible
        .get(state.selected)
        .map(|v| !v.is_parent)
        .unwrap_or(false)
}

/// Performs a Normal-mode [`Action`] on the tree.
///
/// Split out from the key dispatch so a click can run the same code as its key:
/// the tree's click handler maps a clicked row to an `Action` and calls this,
/// exactly as the key dispatch does.
pub(super) fn dispatch_action(action: Action, state: &mut TreeState) {
    match action {
        Action::Quit => {
            state.quit = true;
        }
        Action::ReturnToMenu => {
            state.return_to_menu = true;
            state.quit = true;
        }
        Action::NavDown => {
            state.select_next();
        }
        Action::NavUp => {
            state.select_prev();
        }
        Action::NavRight => {
            if is_leaf_selected(state) {
                state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
            } else {
                state.expand_collapse(true);
            }
        }
        Action::NavLeft => {
            state.collapse_or_select_parent();
        }
        Action::Confirm => {
            if is_leaf_selected(state) {
                state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
            } else {
                state.toggle_expand();
            }
        }
        Action::ToggleExpand => {
            state.toggle_expand();
        }
        Action::Cut => {
            // The Root has no parent and so no sibling slot: `d` is inert there
            // rather than arming a prefix that can only be cancelled.
            if !state.is_root_selected() {
                state.mode = InputMode::PendingCut;
            }
        }
        Action::PasteStart => {
            if state.cut_buffer.is_some() {
                state.mode = InputMode::PendingPaste;
            }
        }
        Action::AddStart => {
            state.mode = InputMode::PendingAdd;
        }
        Action::OpenDetail => {
            state.modal = Some(Modal::Detail(Box::new(Detail::browse())));
        }
        Action::ToggleSearch => {
            state.mode = InputMode::Search;
        }
        Action::ExportStart => {
            state.mode = InputMode::PendingExport;
        }
        Action::Cancel => {
            state.clear_search();
        }
        Action::OpenHelp => {
            state.modal = Some(Modal::Help(Help));
        }
        _ => {}
    }
}

fn handle_pending_cut(key: event::KeyEvent, state: &mut TreeState) {
    if Action::from_pending_cut(&key) == Action::Cut {
        state.cut_selected();
    }
    state.mode = InputMode::Normal;
}

fn handle_pending_paste(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_pending_paste(&key) {
        Action::PasteSibling => {
            state.paste_sibling();
            state.mode = InputMode::Normal;
        }
        Action::PasteBefore => {
            state.paste_sibling_before();
            state.mode = InputMode::Normal;
        }
        Action::PasteChild => {
            state.paste_child();
            state.mode = InputMode::Normal;
        }
        _ => {
            state.mode = InputMode::Normal;
        }
    }
}

pub(super) fn handle_pending_add(key: event::KeyEvent, state: &mut TreeState) {
    // The Root has no parent and so no sibling slot: its sibling targets fall
    // through to the cancel arm, exactly like an unmapped key. The Add form is
    // never opened there, so it cannot become a session that cannot commit.
    let root_selected = state.is_root_selected();
    let position = match Action::from_pending_add(&key) {
        Action::AddSiblingNode if !root_selected => AddPosition::SiblingAfter,
        Action::AddBefore if !root_selected => AddPosition::SiblingBefore,
        Action::AddNode => AddPosition::Child,
        _ => {
            state.mode = InputMode::Normal;
            return;
        }
    };
    state.add_position = position;
    // The Add form is the Detail modal's Form Popup; the prefix mode ends here.
    state.mode = InputMode::Normal;
    state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
}

/// Handles the `Ctrl+E` prefix: `e` opens the export wizard, `s` the share
/// wizard, anything else cancels. Like the `d`/`a`/`p` prefixes, the mode ends
/// whichever key arrives.
fn handle_pending_export(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_pending_export(&key) {
        Action::ExportNode => {
            if let Some(node) = state.current_node_ref() {
                state.modal = Some(Modal::Export(ExportWizard::new(node.clone())));
            }
        }
        Action::ShareNode => {
            if let Some(node) = state.current_node_ref() {
                state.modal = Some(Modal::Share(ShareWizard::new(node.clone())));
            }
        }
        _ => {}
    }
    state.mode = InputMode::Normal;
}

fn handle_search(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_search(&key) {
        Action::ConfirmSearch => {
            state.mode = InputMode::Normal;
        }
        Action::ClearSearch => {
            state.clear_search();
            state.mode = InputMode::Normal;
        }
        Action::Backspace => {
            let mut q = state.search_query.clone();
            q.pop();
            state.set_search(q);
        }
        Action::InsertChar(c) => {
            let mut q = state.search_query.clone();
            q.push(c);
            state.set_search(q);
        }
        _ => {}
    }
}

/// Appends pasted `text` to the search query, single-line only.
///
/// Mirrors the `InsertChar` arm of [`handle_search`], so a paste and typing a
/// character take the same path.
fn handle_search_paste(text: &str, state: &mut TreeState) {
    let cleaned = PastePolicy::SingleLine.clean(text);
    if cleaned.is_empty() {
        return;
    }
    let mut query = state.search_query.clone();
    query.push_str(&cleaned);
    state.set_search(query);
}

/// Applies pasted `text` to the active surface: the open modal, else the search
/// box. Everywhere else — Normal navigation or a prefix mode — it changes
/// nothing.
///
/// The modal is taken out for the duration so it can borrow `state`, mirroring
/// [`handle_modal_key`].
pub(super) fn apply_paste(text: &str, state: &mut TreeState) {
    if let Some(mut modal) = state.modal.take() {
        modal.handle_paste(text, state);
        state.modal = Some(modal);
        return;
    }
    if matches!(state.mode, InputMode::Search) {
        handle_search_paste(text, state);
    }
}

/// Reads the system clipboard and applies it to the active surface.
fn paste_from_clipboard(state: &mut TreeState) {
    paste_into(state, clipboard::paste_text);
}

/// Applies the result of `read` to the active surface.
///
/// An unreadable or empty clipboard is a silent no-op: pasting must never show
/// a false success or disturb the field. When no surface can accept a paste the
/// read is skipped entirely — it is synchronous, and a hung X11 clipboard owner
/// must not freeze the UI for a `Ctrl+V` that provably does nothing. Taking the
/// read as an argument is what lets the failure path be tested without a
/// display.
pub(super) fn paste_into(
    state: &mut TreeState,
    read: impl FnOnce() -> Result<Zeroizing<String>, clipboard::ClipboardError>,
) {
    if !paste_target(state) {
        return;
    }
    if let Ok(text) = read() {
        apply_paste(&text, state);
    }
}

/// Whether the current screen has any surface a paste could land on: an open
/// modal, or the tree search box. Normal navigation and the prefix modes have
/// none.
fn paste_target(state: &TreeState) -> bool {
    state.modal.is_some() || matches!(state.mode, InputMode::Search)
}
