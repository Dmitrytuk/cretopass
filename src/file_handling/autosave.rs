//! Background, coalescing vault autosave.
//!
//! A single writer thread serializes calls to the supplied save function.
//! Submitting a new snapshot replaces any snapshot that has not started yet
//! ("latest wins"), so a burst of edits costs at most one extra save and the
//! newest root always wins.
//!
//! # Failure handling
//!
//! A panic in the save function is caught and reported as [`SaveError::Panicked`]
//! without stopping the worker. A panic in the caller-supplied `on_event`
//! callback stops the worker and closes the event channel, which the application
//! surfaces as [`SaveError::WorkerStopped`].
//!
//! Both caught panics are still printed to stderr by the default panic hook,
//! which can briefly disturb the alternate screen. The hook is intentionally
//! left untouched: it is process-wide state, so mutating it from here would
//! affect unrelated threads and is not worth the risk.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use zeroize::Zeroize;

use super::error::FileHandlingError;

/// Bound on waiting for the save worker to finish.
///
/// Used for the exit flush and for stopping the worker. Long enough to tolerate
/// a slow scrypt run, short enough to return control on a stalled filesystem.
pub const SAVE_TIMEOUT: Duration = Duration::from_secs(10);

/// Error reported by a background save attempt.
///
/// A save is fallible in three distinct ways, and callers can tell them apart
/// without parsing a string: the save function reported a
/// [`FileHandlingError`], it panicked, or the worker stopped before reporting
/// the requested generation.
#[non_exhaustive]
#[derive(Debug)]
pub enum SaveError {
    /// The save function returned a [`FileHandlingError`].
    Failed(FileHandlingError),
    /// The save function panicked; the worker caught it and stayed alive.
    Panicked,
    /// The save thread stopped before reporting the requested generation.
    WorkerStopped,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(error) => error.fmt(f),
            Self::Panicked => f.write_str("save worker panicked"),
            Self::WorkerStopped => f.write_str("save worker stopped"),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failed(error) => Some(error),
            Self::Panicked | Self::WorkerStopped => None,
        }
    }
}

/// Result of a single background save attempt.
pub type SaveResult = Result<(), SaveError>;

/// Notification emitted after a snapshot has been attempted.
#[derive(Debug)]
pub struct SaveEvent {
    /// Monotonic generation assigned by [`Autosave::submit`].
    pub generation: u64,
    /// Whether the snapshot was written successfully.
    pub result: SaveResult,
}

struct Job<T> {
    generation: u64,
    root: T,
}

struct State<T> {
    slot: Option<Job<T>>,
    shutdown: bool,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    available: Condvar,
}

/// Owns the single background writer thread.
///
/// Generic over the snapshot type `T`; the worker treats it as opaque and only
/// hands it to the save function. `T` must be [`Zeroize`] so that a snapshot
/// holding plaintext can be wiped as soon as it is written or replaced.
/// Call [`Autosave::shutdown`] to stop the worker with a bounded wait; the
/// returned error reports only a join timeout. [`Autosave::detach`] stops
/// without waiting, for a force-quit while a save is stuck. [`Drop`] repeats a
/// best-effort bounded join as a fallback for callers that forget.
///
/// Any snapshot still waiting in the slot is written before the thread exits;
/// on timeout the thread is detached and left to be reaped at process exit.
pub struct Autosave<T> {
    shared: Arc<Shared<T>>,
    handle: Option<JoinHandle<()>>,
    next_generation: u64,
}

impl<T: Send + Zeroize + 'static> Autosave<T> {
    /// Starts the writer thread.
    ///
    /// `save` is called on the writer thread with the latest submitted snapshot;
    /// a panic in `save` is caught and reported as [`SaveError::Panicked`].
    /// `on_event` receives one event per attempted snapshot; it must be cheap
    /// and must not panic. A panic in `on_event` stops the worker and closes the
    /// event channel, which the application surfaces as
    /// [`SaveError::WorkerStopped`].
    ///
    /// # Errors
    ///
    /// Returns an error when the writer thread cannot be spawned.
    pub fn spawn<F, G>(save: F, on_event: G) -> std::io::Result<Self>
    where
        F: Fn(&T) -> SaveResult + Send + 'static,
        G: Fn(SaveEvent) + Send + 'static,
    {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                slot: None,
                shutdown: false,
            }),
            available: Condvar::new(),
        });

        let worker_shared = Arc::clone(&shared);
        let handle = thread::Builder::new()
            .name("vault-save".to_string())
            .spawn(move || run_worker(&worker_shared, &save, &on_event))?;

        Ok(Self {
            shared,
            handle: Some(handle),
            next_generation: 0,
        })
    }

    /// Queues `root` for saving and returns its generation.
    ///
    /// If an older snapshot is still waiting, it is replaced: the new snapshot
    /// already contains every earlier change.
    ///
    /// A stopped worker cannot be detected here: this always returns a
    /// generation, even when the worker will never attempt the save. Callers
    /// that need to know should check [`Autosave::is_running`] first (and the
    /// closed event channel reports [`SaveError::WorkerStopped`]).
    pub fn submit(&mut self, root: T) -> u64 {
        self.next_generation += 1;
        let generation = self.next_generation;

        let mut state = lock(&self.shared.state);
        let replaced = state.slot.replace(Job { generation, root });
        drop(state);
        // Drop the replaced snapshot outside the lock: releasing a large tree
        // can be expensive and must not block the worker. Zeroize its plaintext
        // first, since a newer snapshot already supersedes it.
        if let Some(mut job) = replaced {
            job.root.zeroize();
        }
        self.shared.available.notify_one();

        generation
    }
}

impl<T> Autosave<T> {
    /// Returns `true` while the writer thread is alive.
    ///
    /// The worker stops only after a panic in `on_event` has closed the event
    /// channel; from then on [`Autosave::submit`] would accept snapshots that
    /// are never attempted. Callers use this to avoid reporting a pending save
    /// that cannot happen.
    pub fn is_running(&self) -> bool {
        self.handle
            .as_ref()
            .map(|handle| !handle.is_finished())
            .unwrap_or(false)
    }

    /// Stops the worker, waiting up to `timeout` for it to finish.
    ///
    /// Any snapshot still waiting in the slot is written first. When the thread
    /// does not stop within `timeout` it is detached and left to finish on its
    /// own; the returned [`SaveError::WorkerStopped`] reports that timeout.
    pub fn shutdown(mut self, timeout: Duration) -> Result<(), SaveError> {
        self.request_stop();
        match self.handle.take() {
            Some(handle) => join_bounded(handle, timeout),
            None => Ok(()),
        }
    }

    /// Stops the worker without waiting, detaching the thread.
    ///
    /// Used when the user force-quits while a save is stuck: the thread is left
    /// to be reaped at process exit so the UI can return immediately.
    pub fn detach(mut self) {
        self.request_stop();
        // Taking the handle detaches the thread; `Drop` then skips its join.
        self.handle.take();
    }

    /// Signals the worker to exit once the slot is empty.
    fn request_stop(&self) {
        let mut state = lock(&self.shared.state);
        state.shutdown = true;
        self.shared.available.notify_one();
    }
}

impl<T> Drop for Autosave<T> {
    fn drop(&mut self) {
        self.request_stop();

        if let Some(handle) = self.handle.take() {
            // Best-effort bounded join. On timeout the worker is detached and
            // reaped at process exit. A worker panic is reported through its
            // save event when possible; a panic in `on_event` has no receiver
            // left to report to, so the join result adds nothing here.
            let _ = join_bounded(handle, SAVE_TIMEOUT);
        }
    }
}

/// Joins `handle`, giving up after `timeout`.
///
/// On timeout the handle is dropped (detaching the thread) and
/// [`SaveError::WorkerStopped`] is returned. `is_finished` polling is used
/// because `std::thread::JoinHandle` has no timed join. A worker panic is not
/// reported here: it is surfaced through the closed event channel instead, so
/// `shutdown` reports only the join timeout.
fn join_bounded(handle: JoinHandle<()>, timeout: Duration) -> Result<(), SaveError> {
    let deadline = Instant::now() + timeout;
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return Err(SaveError::WorkerStopped);
        }
        thread::sleep(Duration::from_millis(5));
    }
    let _ = handle.join();
    Ok(())
}

/// Runs the writer loop until shutdown is requested and the slot is empty.
fn run_worker<T, F, G>(shared: &Shared<T>, save: &F, on_event: &G)
where
    T: Zeroize,
    F: Fn(&T) -> SaveResult,
    G: Fn(SaveEvent),
{
    loop {
        let mut job = {
            let mut state = lock(&shared.state);
            loop {
                if let Some(job) = state.slot.take() {
                    break job;
                }
                if state.shutdown {
                    return;
                }
                state = shared
                    .available
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        };

        let result =
            catch_unwind(AssertUnwindSafe(|| save(&job.root))).unwrap_or(Err(SaveError::Panicked));

        let event = SaveEvent {
            generation: job.generation,
            result,
        };
        // Wipe the plaintext snapshot now that it has been written.
        job.root.zeroize();
        // A panicking `on_event` must not leave `submit` accepting jobs that are
        // never attempted and never reported: stop the worker so the app sees a
        // closed event channel (`save_rx.recv() -> None`).
        if catch_unwind(AssertUnwindSafe(|| on_event(event))).is_err() {
            return;
        }
    }
}

/// Locks `mutex`, recovering the guard if a panic poisoned it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    use crate::tree::node::Node;

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn node(name: &str) -> Node {
        Node::new(name)
    }

    /// A synthetic file error for tests that need a `SaveError::Failed`.
    fn failed(message: &str) -> SaveError {
        SaveError::Failed(FileHandlingError::Io(std::io::Error::other(message)))
    }

    /// A snapshot that records whether it was zeroized, for the wipe test.
    struct Spy {
        value: String,
        zeroized: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl Zeroize for Spy {
        fn zeroize(&mut self) {
            self.value.zeroize();
            self.zeroized
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[test]
    fn a_superseded_snapshot_is_zeroized() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let save = move |_: &Spy| {
            started_tx.send(()).unwrap();
            release_rx
                .recv_timeout(TIMEOUT)
                .map_err(|_| failed("test release timed out"))?;
            Ok(())
        };
        let (events_tx, _events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(save, move |event| {
            let _ = events_tx.send(event);
        })
        .unwrap();

        let replaced = std::sync::Arc::new(AtomicBool::new(false));
        autosave.submit(Spy {
            value: "first".to_string(),
            zeroized: std::sync::Arc::new(AtomicBool::new(false)),
        });
        started_rx.recv_timeout(TIMEOUT).unwrap();

        // This snapshot waits in the slot...
        autosave.submit(Spy {
            value: "second".to_string(),
            zeroized: std::sync::Arc::clone(&replaced),
        });
        // ...and this one replaces (and so zeroizes) it.
        autosave.submit(Spy {
            value: "third".to_string(),
            zeroized: std::sync::Arc::new(AtomicBool::new(false)),
        });

        assert!(
            replaced.load(Ordering::SeqCst),
            "a superseded plaintext snapshot must be zeroized"
        );

        release_tx.send(()).unwrap();
    }

    #[test]
    fn submit_returns_increasing_generations() {
        let (events_tx, _events_rx) = mpsc::channel();
        let mut autosave = Autosave::spawn(
            |_: &Node| Ok(()),
            move |event| {
                let _ = events_tx.send(event);
            },
        )
        .unwrap();

        assert_eq!(autosave.submit(node("first")), 1);
        assert_eq!(autosave.submit(node("second")), 2);
    }

    #[test]
    fn coalesces_snapshots_while_a_save_is_running() {
        let (started_tx, started_rx) = mpsc::channel::<String>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let save = move |root: &Node| {
            started_tx.send(root.name.clone()).unwrap();
            // Bounded wait so a failed assertion cannot leave the worker (and
            // therefore `Autosave::drop`'s join) blocked forever.
            release_rx
                .recv_timeout(TIMEOUT)
                .map_err(|_| failed("test release timed out"))?;
            Ok(())
        };

        let (events_tx, events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(save, move |event| {
            let _ = events_tx.send(event);
        })
        .unwrap();

        let first = autosave.submit(node("first"));
        assert_eq!(started_rx.recv_timeout(TIMEOUT).unwrap(), "first");

        // The worker is busy with "first"; these two replace each other.
        let second = autosave.submit(node("second"));
        let third = autosave.submit(node("third"));

        release_tx.send(()).unwrap();
        assert_eq!(started_rx.recv_timeout(TIMEOUT).unwrap(), "third");
        release_tx.send(()).unwrap();

        let mut events = Vec::new();
        while events.len() < 2 {
            events.push(events_rx.recv_timeout(TIMEOUT).unwrap());
        }

        assert_eq!(events[0].generation, first);
        assert_eq!(events[1].generation, third);
        assert!(events.iter().all(|event| event.generation != second));
        assert!(events.iter().all(|event| event.result.is_ok()));
    }

    #[test]
    fn failed_save_keeps_the_worker_alive() {
        let save = |root: &Node| {
            if root.name == "bad" {
                Err(failed("disk full"))
            } else {
                Ok(())
            }
        };
        let (events_tx, events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(save, move |event| {
            let _ = events_tx.send(event);
        })
        .unwrap();

        let bad = autosave.submit(node("bad"));
        let event = events_rx.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(event.generation, bad);
        assert!(matches!(event.result, Err(SaveError::Failed(_))));

        let good = autosave.submit(node("good"));
        let event = events_rx.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(event.generation, good);
        assert!(event.result.is_ok());
    }

    #[test]
    fn drop_writes_a_pending_snapshot() {
        let (saved_tx, saved_rx) = mpsc::channel::<String>();
        let save = move |root: &Node| {
            saved_tx.send(root.name.clone()).unwrap();
            Ok(())
        };

        let (events_tx, _events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(save, move |event| {
            let _ = events_tx.send(event);
        })
        .unwrap();

        autosave.submit(node("pending"));
        drop(autosave);

        assert_eq!(saved_rx.recv_timeout(TIMEOUT).unwrap(), "pending");
    }

    #[test]
    fn shutdown_joins_an_idle_worker() {
        let (events_tx, _events_rx) = mpsc::channel::<SaveEvent>();
        let autosave = Autosave::spawn(
            |_: &Node| Ok(()),
            move |event| {
                let _ = events_tx.send(event);
            },
        )
        .unwrap();

        assert!(autosave.shutdown(TIMEOUT).is_ok());
    }

    #[test]
    fn shutdown_reports_a_stuck_worker_without_hanging() {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let save = move |_: &Node| {
            // Block until released; the test asserts the timeout fires first.
            let _ = release_rx.recv_timeout(TIMEOUT);
            Ok(())
        };

        let (events_tx, _events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(save, move |event| {
            let _ = events_tx.send(event);
        })
        .unwrap();

        autosave.submit(node("stuck"));
        let result = autosave.shutdown(Duration::from_millis(50));

        assert!(matches!(result, Err(SaveError::WorkerStopped)));
        // Unblock the detached worker so it does not linger past the test.
        let _ = release_tx.send(());
    }

    #[test]
    fn detach_returns_while_a_save_is_stuck() {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let save = move |_: &Node| {
            let _ = release_rx.recv_timeout(TIMEOUT);
            Ok(())
        };

        let (events_tx, _events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(save, move |event| {
            let _ = events_tx.send(event);
        })
        .unwrap();

        autosave.submit(node("stuck"));
        autosave.detach();

        // Unblock the detached worker so it does not linger past the test.
        let _ = release_tx.send(());
    }

    #[test]
    fn a_panicking_save_is_reported_as_an_error() {
        let (events_tx, events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(
            |_: &Node| -> SaveResult { panic!("boom") },
            move |event| {
                let _ = events_tx.send(event);
            },
        )
        .unwrap();

        let generation = autosave.submit(node("panic"));
        let event = events_rx.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(event.generation, generation);
        assert!(matches!(event.result, Err(SaveError::Panicked)));
    }

    #[test]
    fn drop_writes_the_latest_snapshot_while_one_is_in_flight() {
        let (started_tx, started_rx) = mpsc::channel::<String>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (saved_tx, saved_rx) = mpsc::channel::<String>();

        let save = move |root: &Node| {
            started_tx.send(root.name.clone()).unwrap();
            // Only the first save blocks; the second (queued while the first
            // runs) must complete on its own so the test can assert it.
            if root.name == "first" {
                release_rx
                    .recv_timeout(TIMEOUT)
                    .map_err(|_| failed("test release timed out"))?;
            }
            saved_tx.send(root.name.clone()).unwrap();
            Ok(())
        };

        let (events_tx, _events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(save, move |event| {
            let _ = events_tx.send(event);
        })
        .unwrap();

        // The first save starts and blocks.
        autosave.submit(node("first"));
        assert_eq!(started_rx.recv_timeout(TIMEOUT).unwrap(), "first");

        // A newer snapshot waits in the slot.
        autosave.submit(node("second"));

        // Drop on a separate thread so the join happens off the test thread; the
        // worker is still blocked until we release it.
        let joiner = std::thread::spawn(move || drop(autosave));

        release_tx.send(()).unwrap();

        // Both snapshots are written, newest last, and the join completes.
        assert_eq!(saved_rx.recv_timeout(TIMEOUT).unwrap(), "first");
        assert_eq!(saved_rx.recv_timeout(TIMEOUT).unwrap(), "second");
        joiner.join().unwrap();
    }

    #[test]
    fn a_panicking_on_event_stops_the_worker() {
        let (events_tx, events_rx) = mpsc::channel::<SaveEvent>();
        let mut autosave = Autosave::spawn(
            |_: &Node| Ok(()),
            move |event| {
                // Deliver the event, then panic so the worker stops.
                let _ = events_tx.send(event);
                panic!("on_event boom");
            },
        )
        .unwrap();

        let generation = autosave.submit(node("first"));
        let event = events_rx.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(event.generation, generation);

        // The worker stopped: the event channel is closed, so `submit` can no
        // longer accept jobs with no reporter.
        assert!(matches!(
            events_rx.recv_timeout(TIMEOUT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));

        // ...and it reports itself stopped, so the app can stop showing
        // "Saving" for a save that will never happen.
        let deadline = Instant::now() + TIMEOUT;
        while autosave.is_running() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(!autosave.is_running(), "the worker must report itself dead");
    }

    #[test]
    fn save_error_display_names_each_kind() {
        assert_eq!(
            SaveError::Failed(FileHandlingError::FileTooSmall).to_string(),
            FileHandlingError::FileTooSmall.to_string()
        );
        assert_eq!(SaveError::Panicked.to_string(), "save worker panicked");
        assert_eq!(SaveError::WorkerStopped.to_string(), "save worker stopped");
    }
}
