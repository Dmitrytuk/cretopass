//! The Tree event loop: the async `run_app` driver and its autosave/exit policy.
//!
//! `run_app` owns the terminal for the duration of a Tree session and merges
//! three event sources with `tokio::select!`: terminal input (keys, mouse,
//! paste, resize), the autosave worker's progress, and the spinner tick. The
//! input itself is routed by [`super::input`]; the screen is drawn by
//! [`super::render`].
//!
//! The exit path is the delicate part (see ADR 0010): a committed change is
//! submitted to a background writer, and quitting waits for the latest snapshot,
//! with a bounded timeout that parks a retry/force-quit prompt instead of
//! hanging.

use std::io;

use crate::clipboard;
use crate::file_handling::{
    Autosave, CancelToken, SAVE_TIMEOUT, SaveError, SaveEvent, save_tree_guarded,
};
use crate::theming::SPINNER_TICK;
use crate::tree::node::Node;
use crate::tree::widget::{SaveExit, SaveState, TreeState};
use crossterm::event::{self, Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use zeroize::Zeroizing;

use super::input::{
    apply_paste, apply_paste_result, browser_loading, handle_key_press, handle_mouse,
    poll_browser_listing,
};
use super::render::ui;

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

        // A `Ctrl+V` read runs on a worker thread; this holds the in-flight job
        // so the tick branch can pick its result up without blocking the loop.
        let mut paste: Option<clipboard::PasteJob> = None;

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
                                handle_key_press(
                                    key,
                                    state,
                                    terminal,
                                    &mut autosave,
                                    &mut submitted,
                                    &mut paste,
                                )?;
                            }
                        }
                        Some(Ok(Event::Mouse(mouse))) => {
                            handle_mouse(
                                mouse,
                                state,
                                terminal,
                                &mut autosave,
                                &mut submitted,
                                &mut paste,
                            )?;
                        }
                        Some(Ok(Event::Paste(text))) => {
                            // A term-native paste arrives atomically as one event
                            // once bracketed paste is enabled. It takes the same
                            // path as `Ctrl+V`, but is ignored while a save
                            // overlay owns the screen, exactly like a key press.
                            if state.save_exit == SaveExit::None
                                && state.save_state.overlay_message().is_none()
                            {
                                // The event buffer can hold a secret; wipe it on
                                // drop, like the `Ctrl+V` source does.
                                let text = Zeroizing::new(text);
                                apply_paste(&text, state);
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
                // The tick drives the save spinner and also polls the File
                // Browser's directory listing and an in-flight clipboard read,
                // so a browser waiting on a slow filesystem and a hung
                // clipboard owner both stay off the event loop.
                _ = tick.tick(), if state.save_state.spins() || browser_loading(state) || paste.is_some() => {
                    state.spinner.advance();
                    poll_browser_listing(state);
                    if let Some(result) = paste.as_mut().and_then(clipboard::PasteJob::poll) {
                        paste = None;
                        apply_paste_result(result, state);
                    }
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
pub(super) fn save_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
}

/// What the exit path must do to make the vault durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExitPlan {
    /// Everything submitted has already been written.
    UpToDate,
    /// Wait for the snapshot with this generation.
    Wait(u64),
    /// Re-submit the current root, then wait for it.
    ResubmitThenWait,
}

/// What the autosave loop should do with a modified tree this turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AutosaveAction {
    /// Hand the current snapshot to the worker and show `Saving`.
    Submit,
    /// The worker is gone: report it now instead of waiting for the exit.
    ReportWorkerStopped,
    /// Nothing to do.
    Idle,
}

/// Decides the autosave action for a tree.
///
/// A worker that has stopped (`is_running() == false`) can never write a
/// snapshot, so a modified tree is reported immediately rather than staying
/// silent until the user quits. (Inside the event loop the worker stops only
/// through the closed-event-channel contract of the `file_handling` autosave
/// module, so this is a real failure, not a false alarm.) The report is not
/// repeated once the failure is already showing, so dismissing its overlay
/// keeps it dismissed.
pub(super) fn autosave_action(
    modified: bool,
    worker_running: bool,
    save_state: &SaveState,
) -> AutosaveAction {
    if !modified {
        return AutosaveAction::Idle;
    }
    if worker_running {
        return AutosaveAction::Submit;
    }
    if save_state.has_failed() {
        return AutosaveAction::Idle;
    }
    AutosaveAction::ReportWorkerStopped
}

/// Decides how an exit request should make the vault durable.
pub(super) fn exit_plan(
    modified: bool,
    save_state: &SaveState,
    saved: u64,
    submitted: u64,
) -> ExitPlan {
    if modified || save_state.has_failed() {
        ExitPlan::ResubmitThenWait
    } else if saved < submitted {
        ExitPlan::Wait(submitted)
    } else {
        ExitPlan::UpToDate
    }
}

/// What the save-error overlay did with a key press.
pub(super) enum OverlayAction {
    /// The key was consumed; stay in the Tree.
    Stay,
    /// Re-attempt the exit flush on the next loop iteration.
    Retry,
    /// Leave the Tree without persisting the latest changes.
    Discard,
}

/// Routes a key press while the save-error overlay is showing.
pub(super) fn handle_save_overlay(key: event::KeyEvent, state: &mut TreeState) -> OverlayAction {
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
pub(super) fn start_autosave(
    target: Option<SaveTarget>,
    events: mpsc::UnboundedSender<SaveEvent>,
) -> io::Result<Option<Autosave<Node>>> {
    let Some(SaveTarget { path, passphrase }) = target else {
        return Ok(None);
    };

    // The worker checks this token immediately before its rename, so a save
    // that is still running when the user force-quits cannot land on top of a
    // newer version of the same file.
    let cancel = CancelToken::new();
    let save_cancel = cancel.clone();
    let save = move |root: &Node| {
        save_tree_guarded(root, &path, &passphrase, &save_cancel).map_err(SaveError::Failed)
    };
    let on_event = move |event| {
        // The receiver is dropped when the tree screen closes; a late save
        // notification is then expected and intentionally ignored.
        let _ = events.send(event);
    };

    Autosave::spawn(cancel, save, on_event).map(Some)
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
pub(super) async fn wait_for_generation(
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
