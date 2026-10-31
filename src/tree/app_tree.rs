use std::io;

use crate::file_handling::{Autosave, SAVE_TIMEOUT, SaveError, SaveEvent, save_tree};
use crate::theming::Action;
use crate::theming::{ElementStyle, SPINNER_TICK, paint_background, render_status_bar, texts};
use crate::tree::node::Node;
use crate::tree::widget::{
    AddPosition, Detail, ExportWizard, Help, InputMode, Modal, SaveExit, SaveState, TreeAction,
    TreeState, TreeWidget, centered_rect, execute,
};
use crossterm::event::{self, Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use zeroize::Zeroizing;

/// The vault location and credential the autosave worker needs to persist a
/// tree.
///
/// `path` and `passphrase` are always supplied together, so "path present,
/// passphrase absent" cannot be represented. A tree without a save target (the
/// read-only library demo) runs without autosave.
pub struct SaveTarget {
    /// Path of the secret file to write.
    pub path: String,
    /// Master password used to encrypt the file.
    pub passphrase: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for SaveTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveTarget")
            .field("path", &self.path)
            .field("passphrase", &"<redacted>")
            .finish()
    }
}

/// Outcome of a completed Tree session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// The latest snapshot is on disk (or there was no autosave worker).
    Saved,
    /// The user chose to leave without persisting the latest changes.
    Unsaved,
}

/// Runs the tree screen until the user quits or returns to the home screen.
///
/// Committed changes are handed to a background save worker built from
/// `target`; `None` means a read-only tree (the library demo) with no autosave.
/// On exit the function waits for the latest snapshot to be written. A failed
/// flush parks an exit prompt so the user can retry, quit without saving (after
/// confirmation), or stay; the returned [`ExitStatus`] reports which happened.
///
/// # Errors
///
/// Returns an I/O error when the terminal or event stream fails, or when the
/// save worker cannot be started.
pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
    target: Option<SaveTarget>,
) -> io::Result<ExitStatus> {
    let runtime = save_runtime()?;
    let (save_tx, mut save_rx) = mpsc::unbounded_channel::<SaveEvent>();
    let mut autosave = start_autosave(target, save_tx)?;
    let has_worker = autosave.is_some();

    let mut submitted: u64 = 0;
    let mut saved: u64 = 0;
    let mut save_open = has_worker;

    let status = runtime.block_on(async {
        let mut events = EventStream::new();
        terminal.draw(|f| ui(f, state))?;

        // Drives the title-bar spinner. The first tick is immediate, but the
        // tick branch below is guarded by `save_state.spins()`, so the app does
        // not redraw at the spinner rate while nothing is outstanding.
        let mut tick = tokio::time::interval(SPINNER_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                maybe_event = events.next() => {
                    match maybe_event {
                        Some(Ok(Event::Key(key))) => {
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            if state.save_exit != SaveExit::None
                                || state.save_state.overlay_message().is_some()
                            {
                                match handle_save_overlay(key, state) {
                                    OverlayAction::Discard => break Ok(ExitStatus::Unsaved),
                                    // Retry: fall through so the exit branch re-runs.
                                    OverlayAction::Retry => {}
                                    OverlayAction::Stay => {
                                        terminal.draw(|f| ui(f, state))?;
                                        continue;
                                    }
                                }
                            } else {
                                if state.modal.is_some() {
                                    handle_modal_key(key, state)?;
                                } else {
                                    dispatch_key(key, state);
                                }

                                if state.modified {
                                    state.modified = false;
                                    if let Some(ref mut worker) = autosave {
                                        submitted = worker.submit(state.root.clone());
                                        state.save_state = SaveState::Saving;
                                    }
                                }
                            }
                        }
                        Some(Ok(Event::Resize(_, _))) => {}
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => return Err(e),
                        None => {
                            return Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "terminal event stream ended",
                            ));
                        }
                    }
                }
                event = save_rx.recv(), if save_open => {
                    match event {
                        Some(event) => {
                            if event.result.is_ok() {
                                saved = saved.max(event.generation);
                            }
                            if event.generation == submitted {
                                state.save_state = match event.result {
                                    Ok(()) => SaveState::Idle,
                                    Err(error) => SaveState::Failed {
                                        error,
                                        acknowledged: false,
                                    },
                                };
                            }
                        }
                        None => {
                            save_open = false;
                            if saved < submitted {
                                state.save_state = SaveState::Failed {
                                    error: SaveError::WorkerStopped,
                                    acknowledged: false,
                                };
                            }
                        }
                    }
                }
                _ = tick.tick(), if state.save_state.spins() => {
                    state.spinner.advance();
                }
            }

            if state.save_exit == SaveExit::None && (state.quit || state.return_to_menu) {
                if let Some(ref mut worker) = autosave {
                    let target =
                        match exit_plan(state.modified, &state.save_state, saved, submitted) {
                            ExitPlan::UpToDate => break Ok(ExitStatus::Saved),
                            ExitPlan::Wait(generation) => generation,
                            ExitPlan::ResubmitThenWait => {
                                state.modified = false;
                                worker.submit(state.root.clone())
                            }
                        };

                    state.save_state = SaveState::Saving;
                    terminal.draw(|f| ui(f, state))?;

                    let flush = tokio::time::timeout(
                        SAVE_TIMEOUT,
                        wait_for_save(terminal, state, &mut tick, &mut save_rx, &mut saved, target),
                    )
                    .await;
                    match flush {
                        Ok(result) => match result? {
                            Ok(()) => break Ok(ExitStatus::Saved),
                            Err(error) => park_blocked(state, error),
                        },
                        // The worker is still stuck; offer retry / force-quit.
                        Err(_elapsed) => park_blocked(state, SaveError::WorkerStopped),
                    }
                } else {
                    break Ok(ExitStatus::Saved);
                }
            }

            terminal.draw(|f| ui(f, state))?;
        }
    })?;

    if let Some(worker) = autosave {
        match status {
            // A force-quit may leave a save stuck; detach instead of waiting.
            ExitStatus::Unsaved => worker.detach(),
            ExitStatus::Saved => {
                let _ = worker.shutdown(SAVE_TIMEOUT);
            }
        }
    }

    Ok(status)
}

/// Builds the current-thread runtime that drives the event loop.
///
/// The time driver must be enabled explicitly (it is off by default); the
/// bounded exit flush relies on [`tokio::time::timeout`].
///
/// # Errors
///
/// Returns an error when the runtime cannot be built.
fn save_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
}

/// What the exit path must do to make the vault durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitPlan {
    /// Everything submitted has already been written.
    UpToDate,
    /// Wait for the snapshot with this generation.
    Wait(u64),
    /// Re-submit the current root, then wait for it.
    ResubmitThenWait,
}

/// Decides how an exit request should make the vault durable.
fn exit_plan(modified: bool, save_state: &SaveState, saved: u64, submitted: u64) -> ExitPlan {
    if modified || save_state.has_failed() {
        ExitPlan::ResubmitThenWait
    } else if saved < submitted {
        ExitPlan::Wait(submitted)
    } else {
        ExitPlan::UpToDate
    }
}

/// What the save-error overlay did with a key press.
enum OverlayAction {
    /// The key was consumed; stay in the Tree.
    Stay,
    /// Re-attempt the exit flush on the next loop iteration.
    Retry,
    /// Leave the Tree without persisting the latest changes.
    Discard,
}

/// Routes a key press while the save-error overlay is showing.
fn handle_save_overlay(key: event::KeyEvent, state: &mut TreeState) -> OverlayAction {
    match state.save_exit {
        SaveExit::Blocked => {
            let quit_requested = key.code == KeyCode::Char('q')
                || (key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c'));
            if key.code == KeyCode::Char('r') || key.code == KeyCode::Enter {
                state.save_exit = SaveExit::None;
                OverlayAction::Retry
            } else if quit_requested {
                state.save_exit = SaveExit::ConfirmDiscard;
                OverlayAction::Stay
            } else {
                cancel_exit(state);
                OverlayAction::Stay
            }
        }
        SaveExit::ConfirmDiscard => {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                OverlayAction::Discard
            } else {
                cancel_exit(state);
                OverlayAction::Stay
            }
        }
        SaveExit::None => {
            state.save_state.acknowledge();
            OverlayAction::Stay
        }
    }
}

/// Cancels a parked exit: stay in the Tree and dismiss the overlay.
fn cancel_exit(state: &mut TreeState) {
    state.quit = false;
    state.return_to_menu = false;
    state.save_exit = SaveExit::None;
    state.save_state.acknowledge();
}

/// Parks a failed exit flush behind the retry / quit / stay overlay.
fn park_blocked(state: &mut TreeState, error: SaveError) {
    state.save_exit = SaveExit::Blocked;
    state.save_state = SaveState::Failed {
        error,
        acknowledged: false,
    };
}

/// Builds the background writer when the tree has a save target.
fn start_autosave(
    target: Option<SaveTarget>,
    events: mpsc::UnboundedSender<SaveEvent>,
) -> io::Result<Option<Autosave<Node>>> {
    let Some(SaveTarget { path, passphrase }) = target else {
        return Ok(None);
    };

    let save = move |root: &Node| save_tree(root, &path, &passphrase).map_err(SaveError::Failed);
    let on_event = move |event| {
        // The receiver is dropped when the tree screen closes; a late save
        // notification is then expected and intentionally ignored.
        let _ = events.send(event);
    };

    Autosave::spawn(save, on_event).map(Some)
}

/// Waits until the snapshot with `target` generation has been attempted,
/// animating the title-bar spinner while it waits.
///
/// The inner [`Result`] is the save outcome (as in [`wait_for_generation`]);
/// the outer one reports terminal I/O errors from redrawing the spinner, which
/// are kept distinct from a failed save.
async fn wait_for_save<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
    tick: &mut tokio::time::Interval,
    save_rx: &mut mpsc::UnboundedReceiver<SaveEvent>,
    saved: &mut u64,
    target: u64,
) -> io::Result<Result<(), SaveError>> {
    let wait = wait_for_generation(save_rx, saved, target);
    tokio::pin!(wait);
    loop {
        // Read before the `select!` so the guard does not borrow `state` while
        // the spinner branch mutates it.
        let spinner_on = state.save_state.spins();
        tokio::select! {
            result = &mut wait => return Ok(result),
            _ = tick.tick(), if spinner_on => {
                state.spinner.advance();
                terminal.draw(|f| ui(f, state))?;
            }
        }
    }
}

/// Waits until the snapshot with `target` generation has been attempted.
///
/// Returns the result of that attempt. The caller owns the `SaveState`
/// transition: on success the exit path is finished, so there is nothing left to
/// update. This is the pure core of [`wait_for_save`].
async fn wait_for_generation(
    save_rx: &mut mpsc::UnboundedReceiver<SaveEvent>,
    saved: &mut u64,
    target: u64,
) -> Result<(), SaveError> {
    while let Some(event) = save_rx.recv().await {
        if event.result.is_ok() {
            *saved = (*saved).max(event.generation);
        }
        if event.generation == target {
            return event.result;
        }
    }
    Err(SaveError::WorkerStopped)
}

/// Dispatches a key event to the handler for the current input mode.
fn dispatch_key(key: event::KeyEvent, state: &mut TreeState) {
    match &state.mode {
        InputMode::Normal => handle_normal(key, state),
        InputMode::Search => handle_search(key, state),
        InputMode::PendingCut => handle_pending_cut(key, state),
        InputMode::PendingPaste => handle_pending_paste(key, state),
        InputMode::PendingAdd => handle_pending_add(key, state),
    }
}

/// Dispatches a key to the active modal.
///
/// The modal reads the key itself and reports a [`TreeAction`], which [`execute`]
/// then performs. The modal is taken out of [`TreeState`] for the duration, so an
/// action that dismisses it simply does not put it back.
fn handle_modal_key(key: event::KeyEvent, state: &mut TreeState) -> io::Result<()> {
    let Some(mut modal) = state.modal.take() else {
        return Ok(());
    };

    match modal.handle_key(key, state) {
        // The modal consumed the key and stays open.
        None => {
            state.modal = Some(modal);
            Ok(())
        }
        Some(action) => {
            // The File Browser remembers where it was for its next opening, the
            // same way Home captures it when the browser reports an action.
            if let Modal::ExportDestination(destination) = &modal {
                state.browser_session = destination.session();
            }
            let keeps_produced = !matches!(action, TreeAction::Close);
            let result = execute(state, action);
            state.modal = retained_modal(keeps_produced, modal, state.modal.take());
            result
        }
    }
}

/// The modal showing after a modal action runs.
///
/// A `Close` action leaves the slot as `execute` left it (normally empty). Any
/// other action keeps the modal that produced it, unless `execute` installed a
/// replacement (Export reports its result as an Alert). Keeping the produced
/// modal is what lets the Detail modal stay up to show its transient "copied"
/// alert.
fn retained_modal(
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

fn is_leaf_selected(state: &TreeState) -> bool {
    state
        .visible
        .get(state.selected)
        .map(|v| !v.is_parent)
        .unwrap_or(false)
}

fn handle_normal(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_normal(&key) {
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
        Action::ExportNode => {
            if let Some(node) = state.current_node_ref() {
                state.modal = Some(Modal::Export(ExportWizard::new(node.clone())));
            }
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

fn handle_pending_add(key: event::KeyEvent, state: &mut TreeState) {
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

fn ui(frame: &mut Frame, state: &mut TreeState) {
    let area = frame.area();

    // Own the background: paint SURFACE across the whole screen first so the
    // terminal's own background never shows through.
    paint_background(frame, area);

    let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

    TreeWidget::render(frame, layout[0], state);

    let (mode_str, mode_style, hint) = status_bar_parts(state);
    render_status_bar(frame, layout[1], mode_str, mode_style.style(), &hint);

    if let InputMode::Search = &state.mode {
        render_search_overlay(frame, area, state);
    }

    // The modal is taken out for drawing: the browser's renderer needs `&mut` for
    // its viewport offsets, while the Detail modal reads the tree behind it.
    if let Some(mut modal) = state.modal.take() {
        match &mut modal {
            Modal::Help(help) => help.render(frame, area),
            Modal::Alert(alert) => alert.render(frame, area),
            Modal::Detail(detail) => detail.render(frame, area, state),
            Modal::Export(wizard) => wizard.render(frame, area),
            Modal::ExportDestination(destination) => destination.render(frame, area),
        }
        state.modal = Some(modal);
    }

    match state.save_exit {
        SaveExit::ConfirmDiscard => render_discard_confirm_overlay(frame, area),
        _ => {
            if let Some(message) = state.save_state.overlay_message() {
                render_save_error_overlay(frame, area, &message);
            }
        }
    }
}

/// Maps the active tree mode (or a parked save overlay) to the unified
/// status-bar parts: `(mode label, badge style, context hint)`.
///
/// A parked save overlay takes priority over the mode because it is drawn on
/// top of it. It carries no badge: it is a transient prompt, not a mode.
fn status_bar_parts(state: &TreeState) -> (&'static str, ElementStyle, String) {
    match state.save_exit {
        SaveExit::ConfirmDiscard => {
            return (
                "",
                ElementStyle::ModeNormal,
                texts::HINT_DISCARD_CONFIRM.to_string(),
            );
        }
        SaveExit::Blocked => {
            return (
                "",
                ElementStyle::ModeNormal,
                texts::HINT_SAVE_EXIT_BLOCKED.to_string(),
            );
        }
        SaveExit::None => {}
    }
    if state.save_state.overlay_message().is_some() {
        return (
            "",
            ElementStyle::ModeNormal,
            texts::HINT_DETAIL_ALERT.to_string(),
        );
    }

    if let Some(modal) = &state.modal {
        let (mode, style, hint) = match modal {
            Modal::Help(help) => help.status_parts(),
            Modal::Alert(alert) => alert.status_parts(),
            Modal::Detail(detail) => detail.status_parts(&state.root),
            Modal::Export(wizard) => wizard.status_parts(),
            // The File Browser owns its badge and hint — they follow its mode and
            // the overlay it is showing — so its parts pass through unchanged.
            Modal::ExportDestination(destination) => {
                let (mode, style, hint) = destination.status_parts();
                return (mode, style, hint);
            }
        };
        return (mode, style, hint.to_string());
    }

    let (mode, style, hint) = match &state.mode {
        InputMode::Normal => (
            texts::MODE_NAV,
            ElementStyle::ModeNormal,
            // The Root offers no `d:cut`, so it picks from the root variants.
            match (state.is_root_selected(), state.cut_buffer.is_some()) {
                (false, false) => texts::HINT_TREE,
                (false, true) => texts::HINT_TREE_PASTE,
                (true, false) => texts::HINT_TREE_ROOT,
                (true, true) => texts::HINT_TREE_ROOT_PASTE,
            },
        ),
        InputMode::Search => (
            texts::MODE_SEARCH,
            ElementStyle::ModeSearch,
            texts::HINT_SEARCH,
        ),
        InputMode::PendingCut => (
            texts::MODE_PENDING_CUT,
            ElementStyle::ModePendingCut,
            texts::HINT_PENDING_CUT,
        ),
        InputMode::PendingPaste => (
            texts::MODE_PASTE,
            ElementStyle::ModePendingPaste,
            if state.is_root_selected() {
                texts::HINT_PASTE_ROOT
            } else {
                texts::HINT_PASTE
            },
        ),
        InputMode::PendingAdd => (
            texts::MODE_PENDING_ADD,
            ElementStyle::ModePendingAdd,
            if state.is_root_selected() {
                texts::HINT_PENDING_ADD_ROOT
            } else {
                texts::HINT_PENDING_ADD
            },
        ),
    };
    (mode, style, hint.to_string())
}

fn render_search_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let overlay_area = centered_rect(area, area.width / 2, 3);

    let block = Block::default()
        .title(texts::OVERLAY_SEARCH)
        .borders(Borders::ALL)
        .style(ElementStyle::SearchBlock.style());

    let text = Text::from(Line::from(vec![Span::styled(
        format!("/{}", state.search_query),
        ElementStyle::SearchText.style(),
    )]));

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(text), inner);
}

/// Renders the save-error overlay.
///
/// The retry / quit-without-saving hint and the dismiss hint live in the
/// status bar (see [`status_bar_parts`]); the overlay carries only the message.
fn render_save_error_overlay(frame: &mut Frame, area: Rect, message: &str) {
    let width = (area.width.saturating_mul(2) / 3).max(1);
    let inner_width = width.saturating_sub(2).max(1) as usize;
    // Approximate display width: non-ASCII characters are counted as two
    // cells, covering CJK and most emoji. Include the leading space.
    let display_width = message
        .chars()
        .map(|c| if c.is_ascii() { 1 } else { 2 })
        .sum::<usize>()
        .saturating_add(1);
    let message_rows = display_width.div_ceil(inner_width).max(1) as u16;
    let height = message_rows.saturating_add(3).min(area.height).max(1);

    let overlay_area = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SAVE_ERROR)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", message),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(&block, overlay_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

/// Renders the "quit without saving?" confirmation overlay.
///
/// The confirm/dismiss hint lives in the status bar (see [`status_bar_parts`]).
fn render_discard_confirm_overlay(frame: &mut Frame, area: Rect) {
    let overlay_area = centered_rect(area, area.width * 2 / 3, 4);

    let block = Block::default()
        .title(texts::OVERLAY_SAVE_ERROR)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", texts::MSG_DISCARD_CONFIRM),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(&block, overlay_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    use crate::generator;
    use crate::tree::node::{ExtraField, FieldType};
    use crate::tree::widget::{
        FormFocus, FormPending, FormRow, FormState, GENERATOR_ROW_GENERATE, GENERATOR_ROW_LENGTH,
        GENERATOR_ROW_SYMBOLS, GeneratorModal, PickerItem, form_hint, picker_items,
        render_extra_field_line,
    };

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// A synthetic file error for tests that need a `SaveError::Failed`.
    fn failed(message: &str) -> SaveError {
        SaveError::Failed(crate::file_handling::FileHandlingError::Io(
            io::Error::other(message),
        ))
    }

    /// The Detail modal `state` carries.
    fn detail(state: &TreeState) -> &Detail {
        match &state.modal {
            Some(Modal::Detail(detail)) => detail.as_ref(),
            other => panic!("expected Detail modal, got {other:?}"),
        }
    }

    fn detail_mut(state: &mut TreeState) -> &mut Detail {
        match &mut state.modal {
            Some(Modal::Detail(detail)) => detail.as_mut(),
            other => panic!("expected Detail modal, got {other:?}"),
        }
    }

    /// Drives one key through the same routing the event loop uses: the active
    /// modal first, then the navigation mode.
    fn dispatch_key(key: KeyEvent, state: &mut TreeState) {
        if state.modal.is_some() {
            let _ = super::handle_modal_key(key, state);
        } else {
            super::dispatch_key(key, state);
        }
    }

    fn handle_form(key: KeyEvent, state: &mut TreeState) {
        dispatch_key(key, state);
    }

    fn handle_select_field_type(key: KeyEvent, state: &mut TreeState) {
        dispatch_key(key, state);
    }

    fn handle_confirm_delete(key: KeyEvent, state: &mut TreeState) {
        dispatch_key(key, state);
    }

    fn editing_state() -> TreeState {
        let mut state = TreeState::new(Node::new("Root"));
        let node = state.root.clone();
        state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
        state
    }

    fn editing_state_with_fields() -> TreeState {
        let mut node = Node::new("Root");
        for value in ["a", "b", "c"] {
            node.extra_fields.push(ExtraField {
                field_type: FieldType::Login,
                value: value.to_string(),
            });
        }
        let mut state = TreeState::new(node);
        let node = state.root.clone();
        state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
        state
    }

    fn form_of(state: &TreeState) -> &FormState {
        detail(state).form().expect("the form is open")
    }

    fn form_node(state: &TreeState) -> &Node {
        form_of(state).node(&state.root)
    }

    fn values(state: &TreeState) -> Vec<&str> {
        form_node(state)
            .extra_fields
            .iter()
            .map(|f| f.value.as_str())
            .collect()
    }

    fn select_row(state: &mut TreeState, index: usize) {
        let root = state.root.clone();
        detail_mut(state).form_mut().select_row(&root, index);
    }

    fn type_text(state: &mut TreeState, text: &str) {
        for c in text.chars() {
            dispatch_key(key(KeyCode::Char(c)), state);
        }
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn editing_state_with_password(value: &str) -> TreeState {
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Password,
            value: value.to_string(),
        });
        let mut state = TreeState::new(node);
        let node = state.root.clone();
        state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
        state
    }

    fn password_value(state: &TreeState) -> &str {
        &form_node(state).extra_fields[0].value
    }

    fn generator_modal(state: &TreeState) -> &GeneratorModal {
        detail(state)
            .generator()
            .expect("a generator popup is open")
    }

    fn open_generator(state: &mut TreeState) {
        select_row(state, 1);
        if form_of(state).focus == FormFocus::Nav {
            handle_form(key(KeyCode::Char('i')), state);
        }
        handle_form(ctrl(KeyCode::Char('g')), state);
        assert!(detail(state).generator().is_some());
    }

    fn move_generator_to(state: &mut TreeState, target: usize) {
        while generator_modal(state).selected < target {
            dispatch_key(key(KeyCode::Char('j')), state);
        }
        while generator_modal(state).selected > target {
            dispatch_key(key(KeyCode::Char('k')), state);
        }
    }

    fn set_generator_length(state: &mut TreeState, digits: &str) {
        while !generator_modal(state).length_input.is_empty() {
            dispatch_key(key(KeyCode::Backspace), state);
        }
        for digit in digits.chars() {
            dispatch_key(key(KeyCode::Char(digit)), state);
        }
    }

    fn press_generate(state: &mut TreeState) {
        move_generator_to(state, GENERATOR_ROW_GENERATE);
        dispatch_key(key(KeyCode::Enter), state);
    }

    #[test]
    fn form_opens_in_nav_and_i_enters_input() {
        let mut state = editing_state();
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        handle_form(key(KeyCode::Char('i')), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Input);
    }

    #[test]
    fn form_nav_enter_also_enters_input() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Input);
    }

    #[test]
    fn form_nav_j_moves_row_to_extra_or_add_button() {
        let mut state = editing_state_with_fields();
        handle_form(key(KeyCode::Char('j')), &mut state);
        let form = form_of(&state);
        assert_eq!(form.selected_row, 1); // moved to Extra(0)
        assert_eq!(form.current_row(&state.root), FormRow::Extra(0));
    }

    #[test]
    fn form_nav_esc_closes_to_detail() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Esc), &mut state);
        assert!(matches!(state.modal, Some(Modal::Detail(_))));
    }

    #[test]
    fn empty_add_form_esc_cancels_to_normal() {
        let mut state = TreeState::new(Node::new("Root"));
        state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
        handle_form(key(KeyCode::Esc), &mut state);
        assert!(state.modal.is_none());
        assert!(matches!(state.mode, InputMode::Normal));
    }

    #[test]
    fn overlays_render_on_a_terminal_smaller_than_the_overlay() {
        // A short terminal used to underflow `area.height / 2 - n` and panic
        // while drawing an overlay; every overlay now centres with saturating
        // arithmetic.
        let mut terminal = ratatui::Terminal::new(TestBackend::new(4, 2)).unwrap();

        let mut alert = TreeState::new(Node::new("Root"));
        alert.modal = Some(Modal::alert("boom"));
        terminal.draw(|f| ui(f, &mut alert)).unwrap();

        let mut search = TreeState::new(Node::new("Root"));
        search.mode = InputMode::Search;
        terminal.draw(|f| ui(f, &mut search)).unwrap();

        let mut discard = TreeState::new(Node::new("Root"));
        discard.save_exit = SaveExit::ConfirmDiscard;
        terminal.draw(|f| ui(f, &mut discard)).unwrap();
    }

    #[test]
    fn editing_through_the_modal_router_commits_and_returns_to_normal() {
        // Drive the same routing the event loop uses, end to end:
        // Normal -> Detail -> Form -> commit -> back.
        let mut state = TreeState::new(Node::new("Root"));
        assert!(matches!(state.mode, InputMode::Normal));

        dispatch_key(key(KeyCode::Enter), &mut state); // open Detail (a leaf)
        assert!(matches!(state.modal, Some(Modal::Detail(_))));

        dispatch_key(key(KeyCode::Char('i')), &mut state); // open the Form
        assert!(detail(&state).form().is_some());

        dispatch_key(key(KeyCode::Char('i')), &mut state); // enter Input
        assert_eq!(form_of(&state).focus, FormFocus::Input);

        type_text(&mut state, "!");
        dispatch_key(key(KeyCode::Enter), &mut state); // commit
        assert_eq!(state.root.name, "Root!");
        assert!(state.modified);

        dispatch_key(key(KeyCode::Esc), &mut state); // Form -> browse
        assert!(detail(&state).form().is_none());
        assert!(matches!(state.modal, Some(Modal::Detail(_))));

        dispatch_key(key(KeyCode::Esc), &mut state); // Detail -> Normal
        assert!(state.modal.is_none());
        assert!(matches!(state.mode, InputMode::Normal));
    }

    #[test]
    fn typing_is_committed_on_input_exit() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "!");
        // The node is untouched while typing.
        assert_eq!(form_node(&state).name, "Root");
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_node(&state).name, "Root!");
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        assert!(state.modified);
    }

    #[test]
    fn renaming_a_leaf_updates_the_visible_tree() {
        let leaf = Node::new("Old");
        let leaf_id = leaf.id;
        let root = Node::with_children("Root", vec![leaf]);
        let mut state = TreeState::new(root);
        state.expanded.insert(state.root.id);
        state.update_visible();
        assert_eq!(state.visible.len(), 2);

        state.select_by_id(leaf_id);
        let node = state.current_node_ref().expect("leaf exists").clone();
        state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));

        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "!");
        handle_form(key(KeyCode::Enter), &mut state);

        let visible_leaf = state
            .visible
            .iter()
            .find(|v| v.id == leaf_id)
            .expect("leaf stays visible");
        assert_eq!(visible_leaf.name, "Old!");

        // Undo and redo must also refresh the cached name.
        handle_form(key(KeyCode::Char('u')), &mut state);
        let visible_leaf = state.visible.iter().find(|v| v.id == leaf_id).unwrap();
        assert_eq!(visible_leaf.name, "Old");

        handle_form(key(KeyCode::Char('r')), &mut state);
        let visible_leaf = state.visible.iter().find(|v| v.id == leaf_id).unwrap();
        assert_eq!(visible_leaf.name, "Old!");
    }

    #[test]
    fn leaving_input_unchanged_does_not_commit() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        handle_form(key(KeyCode::Esc), &mut state);
        assert_eq!(form_node(&state).name, "Root");
        assert!(!state.modified);
        assert_eq!(form_of(&state).history.len(), 1);
    }

    #[test]
    fn undo_and_redo_restore_previous_state() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "!");
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_node(&state).name, "Root!");

        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(form_node(&state).name, "Root");
        handle_form(key(KeyCode::Char('r')), &mut state);
        assert_eq!(form_node(&state).name, "Root!");
    }

    #[test]
    fn undo_without_history_is_noop() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(form_node(&state).name, "Root");
        assert!(detail(&state).form().is_some());
    }

    #[test]
    fn new_edit_after_undo_truncates_redo_tail() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "1");
        handle_form(key(KeyCode::Enter), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert!(form_of(&state).can_redo());

        // A new commit discards the redo tail.
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "2");
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_node(&state).name, "Root2");
        assert!(!form_of(&state).can_redo());
    }

    #[test]
    fn add_form_starts_with_only_name_and_add_button() {
        let state = TreeState::new(Node::new("Root"));
        let form = FormState::new_add();
        assert_eq!(
            form.rows(&state.root),
            vec![FormRow::Name, FormRow::AddButton]
        );
    }

    #[test]
    fn edit_form_rows_are_name_fields_and_add_button() {
        let mut node = Node::new("n");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: String::new(),
        });
        let state = TreeState::new(node);
        let form = FormState::new_edit(&state.root);
        assert_eq!(
            form.rows(&state.root),
            vec![FormRow::Name, FormRow::Extra(0), FormRow::AddButton]
        );
    }

    #[test]
    fn add_form_first_commit_creates_the_node() {
        let mut state = TreeState::new(Node::new("Root"));
        state.add_position = AddPosition::Child;
        state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
        assert!(form_of(&state).is_adding());

        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "Child");
        handle_form(key(KeyCode::Enter), &mut state);

        let form = form_of(&state);
        assert!(!form.is_adding());
        let id = form.node_id.expect("node created");
        let created = state.root.find(id).expect("node in tree");
        assert_eq!(created.name, "Child");
        assert_eq!(state.visible[state.selected].id, id);
        assert!(state.modified);
    }

    #[test]
    fn adding_field_via_picker_creates_node_and_enters_input() {
        let mut state = TreeState::new(Node::new("Root"));
        state.add_position = AddPosition::Child;
        state.modal = Some(Modal::Detail(Box::new(Detail::adding())));
        select_row(&mut state, 1); // the + Add Field row

        handle_form(key(KeyCode::Enter), &mut state);
        assert!(detail(&state).is_select_field_type());

        // Custom is offered last; Description sits before it.
        let items = picker_items();
        assert_eq!(items.last(), Some(&PickerItem::Custom));

        let desc_index = items
            .iter()
            .position(|item| *item == PickerItem::Desc)
            .expect("Desc offered");
        detail_mut(&mut state).set_picker_selected(desc_index);
        handle_select_field_type(key(KeyCode::Enter), &mut state);

        let form = form_of(&state);
        assert!(!form.is_adding()); // the first commit created the node
        assert_eq!(form.focus, FormFocus::Input);
        assert_eq!(form.current_row(&state.root), FormRow::Extra(0));
        let node = form_node(&state);
        assert_eq!(node.extra_fields.len(), 1);
        assert_eq!(node.extra_fields[0].field_type.label(), "Description");
        assert_eq!(node.extra_fields[0].value, "");
        assert!(state.modified);
    }

    #[test]
    fn password_is_masked_until_input_level() {
        let ef = ExtraField {
            field_type: FieldType::Password,
            value: "secret".to_string(),
        };
        // Nav: masked.
        let line = render_extra_field_line(&ef, &ef.value, 0, true, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("secret"));
        assert!(text.contains(texts::PASSWORD_MASK));

        // Input: revealed.
        let line = render_extra_field_line(&ef, &ef.value, 0, true, true);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("secret"));
    }

    #[test]
    fn field_cut_requires_two_d_presses_and_is_live() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1); // "a"

        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::Cut);
        assert_eq!(values(&state).len(), 3); // not removed yet

        handle_form(key(KeyCode::Char('d')), &mut state);
        let form = form_of(&state);
        assert_eq!(form.pending, FormPending::None);
        assert_eq!(values(&state), vec!["b", "c"]);
        assert_eq!(form.field_cut_buffer.as_ref().unwrap().value, "a");
    }

    #[test]
    fn field_cut_is_cancelled_by_other_keys() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);

        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Esc), &mut state);
        let form = form_of(&state);
        assert_eq!(form.pending, FormPending::None);
        assert_eq!(values(&state).len(), 3);
        assert!(detail(&state).form().is_some());
    }

    #[test]
    fn field_cut_is_noop_on_name_row() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 0);
        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::None);
    }

    #[test]
    fn field_paste_after_moves_field_down() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1); // "a"

        // Cut "a", then select "b" and paste after it.
        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        select_row(&mut state, 1); // "b"
        handle_form(key(KeyCode::Char('p')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::Paste);
        handle_form(key(KeyCode::Char('p')), &mut state);

        let form = form_of(&state);
        assert_eq!(values(&state), vec!["b", "a", "c"]);
        assert!(form.field_cut_buffer.is_none());
    }

    #[test]
    fn undo_clears_the_field_cut_buffer() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        assert!(form_of(&state).field_cut_buffer.is_some());

        handle_form(key(KeyCode::Char('u')), &mut state);
        assert!(form_of(&state).field_cut_buffer.is_none());
        assert_eq!(values(&state), vec!["a", "b", "c"]);
    }

    #[test]
    fn paste_without_buffer_does_nothing() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        handle_form(key(KeyCode::Char('p')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::None);
        assert_eq!(values(&state).len(), 3);
    }

    #[test]
    fn p_with_empty_buffer_stays_in_normal() {
        let mut state = TreeState::new(Node::new("Root"));
        handle_normal(key(KeyCode::Char('p')), &mut state);
        assert!(matches!(state.mode, InputMode::Normal));
    }

    #[test]
    fn p_with_buffer_enters_pending_paste() {
        let mut state = TreeState::new(Node::new("Root"));
        state.cut_buffer = Some(Node::new("X"));
        handle_normal(key(KeyCode::Char('p')), &mut state);
        assert!(matches!(state.mode, InputMode::PendingPaste));
    }

    /// A state whose root "Root" has one expanded child "Child", so row 0 is
    /// the Root and row 1 a node that has a parent.
    fn state_with_a_child() -> TreeState {
        let root = Node::with_children("Root", vec![Node::new("Child")]);
        let root_id = root.id;
        let mut state = TreeState::new(root);
        state.expanded.insert(root_id);
        state.update_visible();
        state
    }

    #[test]
    fn normal_hint_switches_to_the_root_set() {
        let mut state = state_with_a_child();
        state.selected = 1;
        assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE);

        state.selected = 0;
        assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE_ROOT);
    }

    #[test]
    fn normal_hint_advertises_paste_only_with_a_cut_buffer() {
        let mut state = state_with_a_child();
        state.selected = 1;
        state.cut_buffer = Some(Node::new("X"));
        assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE_PASTE);

        state.selected = 0;
        assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE_ROOT_PASTE);
    }

    #[test]
    fn add_hint_narrows_to_a_child_on_the_root() {
        let mut state = state_with_a_child();
        state.mode = InputMode::PendingAdd;
        state.selected = 1;
        assert_eq!(status_bar_parts(&state).2, texts::HINT_PENDING_ADD);

        state.selected = 0;
        assert_eq!(status_bar_parts(&state).2, texts::HINT_PENDING_ADD_ROOT);
    }

    #[test]
    fn paste_hint_narrows_to_a_child_on_the_root() {
        let mut state = state_with_a_child();
        state.mode = InputMode::PendingPaste;
        state.selected = 1;
        assert_eq!(status_bar_parts(&state).2, texts::HINT_PASTE);

        state.selected = 0;
        assert_eq!(status_bar_parts(&state).2, texts::HINT_PASTE_ROOT);
    }

    #[test]
    fn cut_prefix_is_inert_on_the_root() {
        let mut state = state_with_a_child();
        state.selected = 0;
        handle_normal(key(KeyCode::Char('d')), &mut state);
        assert!(matches!(state.mode, InputMode::Normal));
    }

    #[test]
    fn cut_prefix_is_armed_off_the_root() {
        let mut state = state_with_a_child();
        state.selected = 1;
        handle_normal(key(KeyCode::Char('d')), &mut state);
        assert!(matches!(state.mode, InputMode::PendingCut));
    }

    #[test]
    fn add_sibling_on_the_root_cancels_the_prefix_and_opens_no_form() {
        for sibling in [KeyCode::Char('a'), KeyCode::Char('A')] {
            let mut state = state_with_a_child();
            state.selected = 0;
            handle_normal(key(KeyCode::Char('a')), &mut state);
            assert!(matches!(state.mode, InputMode::PendingAdd));

            handle_pending_add(key(sibling), &mut state);
            assert!(matches!(state.mode, InputMode::Normal));
            assert!(state.modal.is_none());
        }
    }

    #[test]
    fn add_sibling_off_the_root_still_opens_the_form() {
        let mut state = state_with_a_child();
        state.selected = 1;
        handle_normal(key(KeyCode::Char('a')), &mut state);
        handle_pending_add(key(KeyCode::Char('a')), &mut state);
        assert_eq!(state.add_position, AddPosition::SiblingAfter);
        assert!(detail(&state).form().is_some());
    }

    #[test]
    fn add_child_on_the_root_creates_a_child_node() {
        let mut state = state_with_a_child();
        state.selected = 0;
        handle_normal(key(KeyCode::Char('a')), &mut state);
        handle_pending_add(key(KeyCode::Char('l')), &mut state);
        assert_eq!(state.add_position, AddPosition::Child);
        assert!(detail(&state).form().is_some());

        dispatch_key(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "New");
        dispatch_key(key(KeyCode::Enter), &mut state);

        assert!(state.root.children.iter().any(|child| child.name == "New"));
        assert!(state.modified);
    }

    #[test]
    fn tree_hints_stay_within_the_shipped_width() {
        // The status bar is one unwrapped line and the mode badge is drawn in
        // front of the hint, so the hints must not grow past the widest one the
        // Tree already shipped (this is a ceiling, not a terminal-width
        // guarantee).
        const BUDGET: usize = 82;
        let hints = [
            texts::HINT_TREE,
            texts::HINT_TREE_PASTE,
            texts::HINT_TREE_ROOT,
            texts::HINT_TREE_ROOT_PASTE,
            texts::HINT_SEARCH,
            texts::HINT_PENDING_CUT,
            texts::HINT_PASTE,
            texts::HINT_PASTE_ROOT,
            texts::HINT_PENDING_ADD,
            texts::HINT_PENDING_ADD_ROOT,
            texts::HINT_DETAIL,
            texts::HINT_DETAIL_ALERT,
            texts::HINT_FORM,
            texts::HINT_FORM_EXTRA,
            texts::HINT_FORM_EXTRA_PASTE,
            texts::HINT_FORM_ADD,
            texts::HINT_FORM_INPUT,
            texts::HINT_FORM_GENERATOR_INPUT,
            texts::HINT_GENERATOR,
            texts::HINT_SYMBOLS,
            texts::HINT_SELECT_FIELD_TYPE,
            texts::HINT_CUSTOM_FIELD_NAME,
            texts::HINT_CONFIRM_DELETE,
            texts::HINT_FIELD_CUT,
            texts::HINT_FIELD_PASTE,
            texts::HINT_SAVE_EXIT_BLOCKED,
            texts::HINT_DISCARD_CONFIRM,
            texts::HINT_HELP,
        ];
        for hint in hints {
            assert!(
                hint.chars().count() <= BUDGET,
                "hint exceeds the {BUDGET}-char budget: {hint:?}"
            );
        }
    }

    #[test]
    fn confirm_delete_removes_the_field_and_clamps_selection() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 2); // second extra field
        detail_mut(&mut state).set_confirm_delete(1);
        handle_confirm_delete(key(KeyCode::Char('y')), &mut state);
        assert_eq!(values(&state), vec!["a", "c"]);
        assert!(form_of(&state).selected_row < form_of(&state).rows(&state.root).len());
    }

    #[test]
    fn a_copy_action_keeps_the_modal_that_produced_it() {
        // Regression: the routing used to drop the modal whenever it returned a
        // non-Close action, so a copy closed the Detail screen and the "copied"
        // alert never showed.
        let produced = Modal::Detail(Box::new(Detail::browse()));
        assert!(matches!(
            retained_modal(true, produced, None),
            Some(Modal::Detail(_))
        ));
    }

    #[test]
    fn a_close_action_dismisses_the_modal() {
        let produced = Modal::Help(Help);
        assert!(retained_modal(false, produced, None).is_none());
    }

    #[test]
    fn a_modal_installed_by_execute_replaces_the_produced_one() {
        let produced = Modal::Help(Help);
        let installed = Modal::alert("exported");
        assert!(matches!(
            retained_modal(true, produced, Some(installed)),
            Some(Modal::Alert(_))
        ));
    }

    #[test]
    fn exporting_a_node_asks_for_a_path_and_writes_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = TreeState::new(Node::new("Root"));
        state.active_vault_path = Some(dir.path().join("vault.age").to_string_lossy().to_string());

        dispatch_key(ctrl(KeyCode::Char('e')), &mut state); // open the export wizard
        dispatch_key(key(KeyCode::Enter), &mut state); // "this node" -> format
        dispatch_key(key(KeyCode::Enter), &mut state); // JSON -> destination
        assert!(
            matches!(state.modal, Some(Modal::ExportDestination(_))),
            "the format step must open the File Browser"
        );

        dispatch_key(key(KeyCode::Char('e')), &mut state); // write the suggested name

        let written = dir.path().join("vault_Root.json");
        assert!(
            written.exists(),
            "the export must be written where it was named"
        );
        assert!(matches!(state.modal, Some(Modal::Alert(_))));
    }

    #[test]
    fn start_autosave_without_a_target_returns_none() {
        let (tx, _rx) = mpsc::unbounded_channel();
        assert!(start_autosave(None, tx).unwrap().is_none());
    }

    #[test]
    fn runtime_supports_bounded_timeouts() {
        let runtime = save_runtime().unwrap();
        let timed_out = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_millis(1),
                std::future::pending::<()>(),
            )
            .await
        });

        assert!(timed_out.is_err());
    }

    #[test]
    fn exit_plan_resubmits_after_a_failed_save() {
        let save_state = SaveState::Failed {
            error: failed("disk full"),
            acknowledged: false,
        };
        assert_eq!(
            exit_plan(false, &save_state, 1, 2),
            ExitPlan::ResubmitThenWait
        );
    }

    #[test]
    fn exit_plan_waits_for_an_in_flight_snapshot() {
        assert_eq!(
            exit_plan(false, &SaveState::Saving, 1, 2),
            ExitPlan::Wait(2)
        );
    }

    #[test]
    fn exit_plan_is_up_to_date_when_everything_is_written() {
        assert_eq!(exit_plan(false, &SaveState::Idle, 2, 2), ExitPlan::UpToDate);
    }

    #[test]
    fn exit_plan_resubmits_when_a_commit_is_unsubmitted() {
        assert_eq!(
            exit_plan(true, &SaveState::Idle, 2, 2),
            ExitPlan::ResubmitThenWait
        );
    }

    #[test]
    fn blocked_overlay_retries_and_keeps_the_exit_request() {
        let mut state = TreeState::new(Node::new("Root"));
        state.save_exit = SaveExit::Blocked;
        state.quit = true;

        let action = handle_save_overlay(key(KeyCode::Char('r')), &mut state);

        assert!(matches!(action, OverlayAction::Retry));
        assert_eq!(state.save_exit, SaveExit::None);
        assert!(state.quit);
    }

    #[test]
    fn blocked_overlay_confirms_then_discards() {
        let mut state = TreeState::new(Node::new("Root"));
        state.save_exit = SaveExit::Blocked;
        state.return_to_menu = true;

        let action = handle_save_overlay(key(KeyCode::Char('q')), &mut state);
        assert!(matches!(action, OverlayAction::Stay));
        assert_eq!(state.save_exit, SaveExit::ConfirmDiscard);

        let action = handle_save_overlay(key(KeyCode::Char('y')), &mut state);
        assert!(matches!(action, OverlayAction::Discard));
        assert!(state.return_to_menu);
    }

    #[test]
    fn blocked_overlay_other_key_cancels_the_exit() {
        let mut state = TreeState::new(Node::new("Root"));
        state.save_exit = SaveExit::Blocked;
        state.quit = true;

        let action = handle_save_overlay(key(KeyCode::Char('x')), &mut state);

        assert!(matches!(action, OverlayAction::Stay));
        assert_eq!(state.save_exit, SaveExit::None);
        assert!(!state.quit);
    }

    #[tokio::test]
    async fn wait_for_save_reports_the_target_generation() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut saved = 0;

        tx.send(SaveEvent {
            generation: 1,
            result: Ok(()),
        })
        .unwrap();
        tx.send(SaveEvent {
            generation: 3,
            result: Ok(()),
        })
        .unwrap();

        let result = wait_for_generation(&mut rx, &mut saved, 3).await;

        assert!(result.is_ok());
        assert_eq!(saved, 3);
    }

    #[tokio::test]
    async fn wait_for_save_keeps_the_last_successful_generation_on_error() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut saved = 0;

        tx.send(SaveEvent {
            generation: 1,
            result: Ok(()),
        })
        .unwrap();
        tx.send(SaveEvent {
            generation: 2,
            result: Err(failed("disk full")),
        })
        .unwrap();

        let result = wait_for_generation(&mut rx, &mut saved, 2).await;

        assert!(matches!(result, Err(SaveError::Failed(_))));
        assert_eq!(saved, 1);
    }

    #[tokio::test]
    async fn wait_for_save_detects_a_stopped_worker() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut saved = 0;

        let result = wait_for_generation(&mut rx, &mut saved, 1).await;

        assert!(matches!(result, Err(SaveError::WorkerStopped)));
    }

    #[test]
    fn generator_ctrl_g_is_bound_only_while_editing_a_password_field() {
        // `Ctrl+G` is unbound at the Nav level, even on a password row.
        let mut nav = editing_state_with_password("old");
        select_row(&mut nav, 1);
        handle_form(ctrl(KeyCode::Char('g')), &mut nav);
        assert!(detail(&nav).form().is_some());

        // At the Input level it opens only when the focused field carries a
        // generator: Login and Name rows are no-ops.
        let mut login = editing_state_with_fields();
        select_row(&mut login, 1);
        handle_form(key(KeyCode::Char('i')), &mut login);
        handle_form(ctrl(KeyCode::Char('g')), &mut login);
        assert!(detail(&login).form().is_some());

        let mut name = editing_state();
        select_row(&mut name, 0);
        handle_form(key(KeyCode::Char('i')), &mut name);
        handle_form(ctrl(KeyCode::Char('g')), &mut name);
        assert!(detail(&name).form().is_some());

        let mut password = editing_state_with_password("old");
        open_generator(&mut password);
    }

    #[test]
    fn generator_generate_writes_commits_and_returns_to_input_form() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        set_generator_length(&mut state, "12");
        press_generate(&mut state);

        assert!(detail(&state).form().is_some());
        assert_eq!(form_of(&state).focus, FormFocus::Input);
        let value = password_value(&state);
        assert_eq!(value.chars().count(), 12);
        assert!(value.chars().all(|c| c.is_ascii_graphic()));
        assert!(state.modified);
        assert_eq!(form_of(&state).history.len(), 2);
        assert_eq!(state.generator.length.value(), 12);

        // The scratch buffer mirrors the generated value, so leaving the input
        // level keeps the password instead of restoring the old one.
        let generated = value.to_string();
        assert_eq!(
            form_of(&state).scratch.as_ref().map(|s| s.text.as_str()),
            Some(generated.as_str())
        );
        handle_form(key(KeyCode::Esc), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        assert_eq!(password_value(&state), generated);
    }

    #[test]
    fn generated_password_is_undoable() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        press_generate(&mut state);
        assert_ne!(password_value(&state), "old");

        // Generation leaves the form at the Input level, where `u` is plain
        // text; leave it first so `u` reaches the Nav-level undo.
        handle_form(key(KeyCode::Esc), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(password_value(&state), "old");
    }

    #[test]
    fn generator_esc_leaves_the_value_untouched() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        dispatch_key(key(KeyCode::Esc), &mut state);

        assert!(detail(&state).form().is_some());
        assert_eq!(password_value(&state), "old");
        assert!(!state.modified);
        assert_eq!(form_of(&state).history.len(), 1);
    }

    #[test]
    fn generator_out_of_range_length_shows_an_error_and_does_not_write() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        set_generator_length(&mut state, "999");
        press_generate(&mut state);

        assert!(detail(&state).generator().is_some());
        assert!(!detail(&state).is_symbol_checklist());
        assert!(generator_modal(&state).error.is_some());
        let value = &generator_modal(&state).form.node(&state.root).extra_fields[0].value;
        assert_eq!(value, "old");
        assert!(!state.modified);
    }

    #[test]
    fn generator_digits_replace_the_seed_and_edit_at_the_caret() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);

        // The seeded "10" is replaced by the first digits typed.
        dispatch_key(key(KeyCode::Char('1')), &mut state);
        dispatch_key(key(KeyCode::Char('2')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "12");
        assert_eq!(generator_modal(&state).length_cursor, 2);

        // The caret can be moved and a digit inserted mid-string.
        dispatch_key(key(KeyCode::Left), &mut state);
        assert_eq!(generator_modal(&state).length_cursor, 1);
        dispatch_key(key(KeyCode::Char('0')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "102");

        // Home / End move to the edges; Delete removes at the caret.
        dispatch_key(key(KeyCode::Home), &mut state);
        assert_eq!(generator_modal(&state).length_cursor, 0);
        dispatch_key(key(KeyCode::Delete), &mut state);
        assert_eq!(generator_modal(&state).length_input, "02");
        dispatch_key(key(KeyCode::End), &mut state);
        assert_eq!(generator_modal(&state).length_cursor, 2);

        // At most three digits are kept.
        dispatch_key(key(KeyCode::Char('7')), &mut state);
        dispatch_key(key(KeyCode::Char('8')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "027");
    }

    #[test]
    fn generator_length_backspace_on_fresh_seed_clears_it() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);

        dispatch_key(key(KeyCode::Backspace), &mut state);
        assert_eq!(generator_modal(&state).length_input, "");
        assert_eq!(generator_modal(&state).length_cursor, 0);

        dispatch_key(key(KeyCode::Char('4')), &mut state);
        dispatch_key(key(KeyCode::Char('2')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "42");
    }

    #[test]
    fn generator_digits_and_caret_keys_only_edit_the_length_row() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_GENERATE);

        for code in [
            KeyCode::Char('5'),
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Left,
            KeyCode::Home,
        ] {
            dispatch_key(key(code), &mut state);
        }
        assert_eq!(generator_modal(&state).length_input, "10");
        assert_eq!(generator_modal(&state).length_cursor, 2);
    }

    #[test]
    fn generator_enter_on_length_moves_to_symbols_row() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_LENGTH);

        dispatch_key(key(KeyCode::Enter), &mut state);
        assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_SYMBOLS);
    }

    #[test]
    fn generator_symbols_summary_toggles_all() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);

        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert!(generator_modal(&state).params.symbols.is_empty());
        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert_eq!(
            generator_modal(&state).params.symbols,
            generator::SymbolSet::all()
        );
    }

    #[test]
    fn symbol_checklist_moves_in_a_grid_and_toggles() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);
        dispatch_key(key(KeyCode::Enter), &mut state);
        assert!(detail(&state).is_symbol_checklist());

        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert!(!generator_modal(&state).params.symbols.contains('!'));
        dispatch_key(key(KeyCode::Char('l')), &mut state);
        assert_eq!(generator_modal(&state).selected, 1);
        dispatch_key(key(KeyCode::Char('j')), &mut state);
        assert_eq!(generator_modal(&state).selected, 4);
        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert!(!generator_modal(&state).params.symbols.contains('%'));

        dispatch_key(key(KeyCode::Esc), &mut state);
        assert!(detail(&state).generator().is_some());
        assert!(!detail(&state).is_symbol_checklist());
        assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_SYMBOLS);
    }

    #[test]
    fn generator_with_all_symbols_off_generates_alphanumerics() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);
        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        press_generate(&mut state);

        assert!(detail(&state).form().is_some());
        let value = password_value(&state);
        assert!(value.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(value.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn generator_settings_are_remembered_for_the_next_open() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        set_generator_length(&mut state, "12");
        press_generate(&mut state);

        open_generator(&mut state);
        assert_eq!(generator_modal(&state).length_input, "12");
    }

    #[test]
    fn form_hint_offers_the_generator_only_while_editing_a_generated_field() {
        let mut password = editing_state_with_password("old");
        select_row(&mut password, 1);
        assert_eq!(
            form_hint(form_of(&password), &password.root),
            texts::HINT_FORM_EXTRA
        );
        handle_form(key(KeyCode::Char('i')), &mut password);
        assert_eq!(
            form_hint(form_of(&password), &password.root),
            texts::HINT_FORM_GENERATOR_INPUT
        );

        let mut login = editing_state_with_fields();
        select_row(&mut login, 1);
        assert_eq!(
            form_hint(form_of(&login), &login.root),
            texts::HINT_FORM_EXTRA
        );
        handle_form(key(KeyCode::Char('i')), &mut login);
        assert_eq!(
            form_hint(form_of(&login), &login.root),
            texts::HINT_FORM_GENERATOR_INPUT
        );

        // A Custom field has no generator, so its hint stays the plain input
        // one at the Input level.
        let mut custom = editing_state();
        custom.root.extra_fields.push(ExtraField {
            field_type: FieldType::Custom("URL".to_string()),
            value: String::new(),
        });
        let node = custom.root.clone();
        custom.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
        select_row(&mut custom, 1);
        handle_form(key(KeyCode::Char('i')), &mut custom);
        assert_eq!(
            form_hint(form_of(&custom), &custom.root),
            texts::HINT_FORM_INPUT
        );

        let mut name = editing_state();
        select_row(&mut name, 0);
        assert_eq!(form_hint(form_of(&name), &name.root), texts::HINT_FORM);
    }

    #[test]
    fn form_extra_hint_advertises_paste_only_with_a_field_buffer() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        assert_eq!(
            form_hint(form_of(&state), &state.root),
            texts::HINT_FORM_EXTRA
        );

        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(
            form_hint(form_of(&state), &state.root),
            texts::HINT_FORM_EXTRA_PASTE
        );
    }

    fn open_login_row(state: &mut TreeState) {
        select_row(state, 1);
        if form_of(state).focus == FormFocus::Nav {
            handle_form(key(KeyCode::Char('i')), state);
        }
    }

    fn generate_login_word(state: &mut TreeState) {
        assert_eq!(form_of(state).focus, FormFocus::Input);
        handle_form(ctrl(KeyCode::Char('g')), state);
    }

    #[test]
    fn ctrl_g_on_a_login_row_writes_a_word_without_opening_a_modal() {
        let mut state = editing_state_with_fields();
        open_login_row(&mut state);
        generate_login_word(&mut state);

        // No generator popup: the form is still active at the Input level.
        assert!(detail(&state).form().is_some());
        assert_eq!(form_of(&state).focus, FormFocus::Input);

        let value = values(&state)[0];
        assert_ne!(value, "a");
        assert!(value.chars().all(|c| c.is_ascii_lowercase()));
        assert!((4..=13).contains(&value.chars().count()));
        assert!(state.modified);
        // The scratch buffer mirrors the value, so leaving Input keeps it.
        assert_eq!(
            form_of(&state).scratch.as_ref().map(|s| s.text.as_str()),
            Some(value)
        );
        let generated = value.to_string();
        handle_form(key(KeyCode::Esc), &mut state);
        assert_eq!(values(&state)[0], generated);
    }

    #[test]
    fn login_reroll_coalesces_into_one_undo_step() {
        let mut state = editing_state_with_fields();
        open_login_row(&mut state);
        generate_login_word(&mut state);
        assert_eq!(form_of(&state).history.len(), 2);

        generate_login_word(&mut state);
        // The reroll replaced the previous generation instead of stacking.
        assert_eq!(form_of(&state).history.len(), 2);
        assert!(values(&state)[0].chars().all(|c| c.is_ascii_lowercase()));

        // One undo restores the pre-generation value.
        handle_form(key(KeyCode::Esc), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(values(&state)[0], "a");
    }

    #[test]
    fn ctrl_g_outside_a_generated_field_is_a_noop() {
        // Name row at the Input level.
        let mut state = editing_state();
        select_row(&mut state, 0);
        handle_form(key(KeyCode::Char('i')), &mut state);
        handle_form(ctrl(KeyCode::Char('g')), &mut state);
        assert!(!state.modified);
        assert_eq!(form_of(&state).history.len(), 1);

        // A Login row at the Nav level: `Ctrl+G` is unbound there.
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        handle_form(ctrl(KeyCode::Char('g')), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        assert!(!state.modified);
        assert_eq!(values(&state)[0], "a");
    }

    #[test]
    fn password_reroll_also_coalesces_into_one_undo_step() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        press_generate(&mut state);
        assert_eq!(form_of(&state).history.len(), 2);

        open_generator(&mut state);
        press_generate(&mut state);
        assert_eq!(form_of(&state).history.len(), 2);

        handle_form(key(KeyCode::Esc), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(password_value(&state), "old");
    }

    /// Renders `ui` into a test backend and asserts that no cell falls through
    /// to the terminal's own background (see
    /// [ADR 0018](../../../../docs/adr/0018-app-owned-background-fill.md)).
    fn assert_no_terminal_background(state: &mut TreeState) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| ui(frame, state)).unwrap();

        let buffer = terminal.backend().buffer();
        for (i, cell) in buffer.content.iter().enumerate() {
            assert_ne!(
                cell.style().bg,
                Some(Color::Reset),
                "cell {i} ({:?}) fell through to the terminal background",
                cell.symbol()
            );
        }
    }

    #[test]
    fn tree_screen_paints_every_cell() {
        let mut state = TreeState::new(crate::build_demo_tree());
        assert_no_terminal_background(&mut state);
    }

    #[test]
    fn search_overlay_paints_every_cell() {
        // The Search overlay clears its rect before drawing the block, the path
        // most likely to expose the terminal background.
        let mut state = TreeState::new(Node::new("Root"));
        state.mode = InputMode::Search;
        state.search_query = "Ro".to_string();
        assert_no_terminal_background(&mut state);
    }
}
