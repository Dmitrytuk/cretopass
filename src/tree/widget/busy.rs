//! A blocking Tree operation drawn with an animated title-bar status.
//!
//! The Tree mirrors Home's `run_with_spinner` ([ADR 0011]): the work runs on its
//! own thread while the UI thread keeps redrawing every [`SPINNER_TICK`], so the
//! `Saving` glyph advances for the whole write instead of freezing on the first
//! frame. The status lives in [`TreeState::operation_status`], which takes
//! title-bar precedence over the autosave `save_state`.
//!
//! Unlike Home, the wait does not drain input: the Tree's keys arrive through a
//! tokio `EventStream` with its own reader thread, so polling crossterm here
//! would race it. Keys pressed during the wait are buffered and handled on the
//! next turn of the event loop.
//!
//! [ADR 0011]: ../../../docs/adr/0011-animated-status-spinner.md

use std::io;
use std::thread;

use ratatui::Frame;
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::theming::{SPINNER_TICK, Status};

use super::TreeState;

/// Runs `work` on a worker thread while `status` animates in the title bar,
/// redrawing with `draw` every [`SPINNER_TICK`].
///
/// `draw` is the screen's full-frame renderer (`ui` in `app_tree`), passed in so
/// this module need not know how the Tree is assembled. `status` is cleared on
/// every return path — success, error, or a worker panic alike — without a final
/// draw; the caller installs the result modal and the event loop's next draw
/// renders it, exactly as Home's helper does.
///
/// # Errors
///
/// Returns an I/O error when the worker thread cannot be spawned, the terminal
/// cannot be redrawn mid-wait, or the worker panics. A redraw failure does not
/// cancel the worker: the write is non-cancellable, so the error is returned only
/// once the worker has finished.
pub(crate) fn run_with_busy<B, T>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
    status: Status,
    draw: fn(&mut Frame, &mut TreeState),
    work: impl FnOnce() -> T + Send + 'static,
) -> io::Result<T>
where
    B: Backend,
    T: Send + 'static,
{
    state.operation_status = Some(status);
    let outcome = run_animated(terminal, state, draw, work);
    // Cleared here rather than in `run_animated`, so a failed draw, a failed
    // spawn or a worker panic cannot leave the title bar pinned on `Saving`.
    state.operation_status = None;
    outcome
}

/// The animation loop: spawns `work`, redraws every [`SPINNER_TICK`] until it
/// finishes, and joins it.
fn run_animated<B, T>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
    draw: fn(&mut Frame, &mut TreeState),
    work: impl FnOnce() -> T + Send + 'static,
) -> io::Result<T>
where
    B: Backend,
    T: Send + 'static,
{
    let handle = thread::Builder::new()
        .spawn(work)
        .map_err(io::Error::other)?;

    // A failed draw is remembered, not returned: the worker is non-cancellable,
    // so we keep redrawing (best effort) and only surface the error once the
    // worker has finished.
    let mut draw_error = terminal.draw(|f| draw(f, state)).err();
    while !handle.is_finished() {
        state.spinner.advance();
        if draw_error.is_none() {
            draw_error = terminal.draw(|f| draw(f, state)).err();
        }
        thread::sleep(SPINNER_TICK);
    }

    let result = handle
        .join()
        .map_err(|_| io::Error::other("blocking worker thread panicked"))?;
    match draw_error {
        Some(error) => Err(error),
        None => Ok(result),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::Node;
    use ratatui::backend::TestBackend;

    fn no_draw(_frame: &mut Frame, _state: &mut TreeState) {}

    #[test]
    fn the_status_is_cleared_and_the_work_value_returned() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let mut state = TreeState::new(Node::new("Root"));

        let value =
            run_with_busy(&mut terminal, &mut state, Status::Saving, no_draw, || 42).unwrap();

        assert_eq!(value, 42);
        assert!(
            state.operation_status.is_none(),
            "the busy status must be cleared on return"
        );
    }
}
