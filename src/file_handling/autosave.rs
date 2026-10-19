//! Background, coalescing vault autosave.
//!
//! A single writer thread serializes calls to the supplied save function.
//! Submitting a new snapshot replaces any snapshot that has not started yet
//! ("latest wins"), so a burst of edits costs at most one extra save and the
//! newest root always wins.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use crate::tree::node::Node;

/// Error reported by a background save attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The save function returned an error.
    Failed(String),
    /// The save function panicked; the worker caught it and stayed alive.
    Panicked,
    /// The save thread stopped before reporting the requested generation.
    WorkerStopped,
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed(message) => f.write_str(message),
            Self::Panicked => f.write_str("save worker panicked"),
            Self::WorkerStopped => f.write_str("save worker stopped"),
        }
    }
}

impl std::error::Error for SaveError {}

/// Result of a single background save attempt.
pub type SaveResult = Result<(), SaveError>;

/// Notification emitted after a snapshot has been attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveEvent {
    /// Monotonic generation assigned by [`Autosave::submit`].
    pub generation: u64,
    /// Whether the snapshot was written successfully.
    pub result: SaveResult,
}

struct Job {
    generation: u64,
    root: Node,
}

struct State {
    slot: Option<Job>,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    available: Condvar,
}

/// Owns the single background writer thread.
///
/// The worker is stopped and joined on drop. Any snapshot still waiting in the
/// slot is written before the thread exits.
pub struct Autosave {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
    next_generation: u64,
}

impl Autosave {
    /// Starts the writer thread.
    ///
    /// `save` is called on the writer thread with the latest submitted root.
    /// `on_event` receives one event per attempted snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the writer thread cannot be spawned.
    pub fn spawn<F, G>(save: F, on_event: G) -> std::io::Result<Self>
    where
        F: Fn(&Node) -> SaveResult + Send + 'static,
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
    pub fn submit(&mut self, root: Node) -> u64 {
        self.next_generation += 1;
        let generation = self.next_generation;

        let mut state = lock(&self.shared.state);
        state.slot = Some(Job { generation, root });
        self.shared.available.notify_one();

        generation
    }
}

impl Drop for Autosave {
    fn drop(&mut self) {
        {
            let mut state = lock(&self.shared.state);
            state.shutdown = true;
            self.shared.available.notify_one();
        }

        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Runs the writer loop until shutdown is requested and the slot is empty.
fn run_worker<F, G>(shared: &Shared, save: &F, on_event: &G)
where
    F: Fn(&Node) -> SaveResult,
    G: Fn(SaveEvent),
{
    loop {
        let job = {
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

        on_event(SaveEvent {
            generation: job.generation,
            result,
        });
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

    const TIMEOUT: Duration = Duration::from_secs(5);

    fn node(name: &str) -> Node {
        Node::new(name)
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
            release_rx.recv().unwrap();
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
        assert!(events.iter().all(|event| event.result == Ok(())));
    }

    #[test]
    fn failed_save_keeps_the_worker_alive() {
        let save = |root: &Node| {
            if root.name == "bad" {
                Err(SaveError::Failed("disk full".to_string()))
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
        assert_eq!(
            event.result,
            Err(SaveError::Failed("disk full".to_string()))
        );

        let good = autosave.submit(node("good"));
        let event = events_rx.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(event.generation, good);
        assert_eq!(event.result, Ok(()));
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
        assert_eq!(event.result, Err(SaveError::Panicked));
    }
}
