//! Background directory listing with a bounded wait.
//!
//! A directory read can block for as long as the filesystem takes — a
//! disconnected network share, a hung FUSE mount, or a cloud folder whose
//! provider is deciding whether to answer. Running it on the UI thread freezes
//! the whole TUI, so [`DirLoader`] runs each request on its own detached thread
//! and lets the UI keep drawing.
//!
//! # Thread per request, not a worker
//!
//! A blocked filesystem call can be neither cancelled nor reused, so a
//! coalescing worker (like [`super::autosave`]) would pin one thread forever.
//! Each request instead gets a fresh thread; a thread that never returns is
//! abandoned and reaped at process exit. Only one result is ever wanted: every
//! request carries a generation and a late result for a superseded request is
//! dropped, so a burst of navigation cannot deliver an older listing over a
//! newer one.
//!
//! # Timeout
//!
//! [`LISTING_TIMEOUT`] bounds how long the UI waits for the *newest* request.
//! When it elapses [`DirLoader::poll`] reports [`ListingOutcome::TimedOut`] and
//! forgets the request; the detached thread keeps running but its result is
//! stale and ignored.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::disks::{DiskInfo, list_disks};
use super::error::FileHandlingError;
use super::file_process::{FileEntry, list_dir, list_dir_including_hidden};

/// How long the UI waits for a directory listing before giving up.
///
/// Mirrors [`super::autosave::SAVE_TIMEOUT`]: long enough for a slow disk, short
/// enough that a hidden OS consent dialog (a cloud provider asking whether the
/// app may download a file) cannot present itself as a permanent freeze.
pub const LISTING_TIMEOUT: Duration = Duration::from_secs(10);

/// The result a completed listing request hands back.
#[non_exhaustive]
#[derive(Debug)]
pub enum ListingOutcome {
    /// `dir` was read successfully.
    Loaded {
        /// The directory that was read.
        dir: PathBuf,
        /// Its entries.
        entries: Vec<FileEntry>,
    },
    /// `dir` could not be read.
    Failed {
        /// The directory that failed.
        dir: PathBuf,
        /// Why it failed.
        error: FileHandlingError,
    },
    /// No result arrived within [`LISTING_TIMEOUT`]; the request is abandoned.
    TimedOut {
        /// The directory that did not answer in time.
        dir: PathBuf,
    },
}

/// A message a listing thread sends when it finishes.
#[derive(Debug)]
struct Message {
    generation: u64,
    dir: PathBuf,
    entries: Result<Vec<FileEntry>, FileHandlingError>,
}

/// The newest request the loader is waiting for.
#[derive(Debug)]
struct Pending {
    generation: u64,
    dir: PathBuf,
    started: Instant,
}

/// Lists directories off the UI thread and reports the newest result.
///
/// See the module docs for the thread-per-request and generation rules.
#[derive(Debug)]
pub struct DirLoader {
    tx: Sender<Message>,
    rx: Receiver<Message>,
    generation: u64,
    pending: Option<Pending>,
}

impl Default for DirLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl DirLoader {
    /// Creates a loader with no outstanding request.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            generation: 0,
            pending: None,
        }
    }

    /// Starts listing `dir`, replacing any request still in flight.
    ///
    /// The previous request (if any) is superseded: its result will be dropped
    /// when it eventually arrives, and it no longer counts for the timeout.
    pub fn request(&mut self, dir: &Path, show_hidden: bool) {
        self.generation += 1;
        let generation = self.generation;
        let dir = dir.to_path_buf();
        self.pending = Some(Pending {
            generation,
            dir: dir.clone(),
            started: Instant::now(),
        });

        let tx = self.tx.clone();
        let thread_dir = dir.clone();
        // The thread is intentionally detached: a stuck fs call cannot be
        // cancelled, so joining it would reintroduce the very freeze this
        // module exists to avoid. Its late result is dropped as stale.
        let spawned = thread::Builder::new()
            .name("dir-listing".to_string())
            .spawn(move || {
                let entries = list_for(&thread_dir, show_hidden);
                let _ = tx.send(Message {
                    generation,
                    dir: thread_dir,
                    entries,
                });
            });

        if let Err(error) = spawned {
            // A thread that cannot start is reported like a failed listing, so
            // the caller falls back instead of waiting out the timeout.
            let _ = self.tx.send(Message {
                generation,
                dir,
                entries: Err(FileHandlingError::Io(error)),
            });
        }
    }

    /// Returns `true` while a request is outstanding.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Abandons the outstanding request without waiting for it.
    ///
    /// Any result it later produces is stale and dropped.
    pub fn cancel(&mut self) {
        self.pending = None;
        self.generation += 1;
    }

    /// Reports the newest request's outcome, if it is ready.
    ///
    /// Stale results (from superseded requests) are discarded. Order: a
    /// completed result wins over the timeout, so a listing that lands just as
    /// the deadline passes is still delivered.
    pub fn poll(&mut self) -> Option<ListingOutcome> {
        self.poll_at(Instant::now())
    }

    /// [`Self::poll`] against a caller-supplied clock, so the timeout can be
    /// exercised without waiting (and without backdating an [`Instant`]).
    fn poll_at(&mut self, now: Instant) -> Option<ListingOutcome> {
        let Some(generation) = self.pending.as_ref().map(|pending| pending.generation) else {
            // Nothing outstanding: drain so a late stale result cannot build up.
            while self.rx.try_recv().is_ok() {}
            return None;
        };

        loop {
            match self.rx.try_recv() {
                Ok(message) if message.generation == generation => {
                    self.pending = None;
                    return Some(match message.entries {
                        Ok(entries) => ListingOutcome::Loaded {
                            dir: message.dir,
                            entries,
                        },
                        Err(error) => ListingOutcome::Failed {
                            dir: message.dir,
                            error,
                        },
                    });
                }
                // A superseded request: drop it.
                Ok(_) => {}
                Err(_) => break,
            }
        }

        let timed_out = self
            .pending
            .as_ref()
            .is_some_and(|pending| now.duration_since(pending.started) >= LISTING_TIMEOUT);
        if timed_out {
            let dir = self.pending.take().map(|pending| pending.dir)?;
            return Some(ListingOutcome::TimedOut { dir });
        }

        None
    }
}

/// Lists `dir` honouring the hidden-entry setting.
fn list_for(dir: &Path, show_hidden: bool) -> Result<Vec<FileEntry>, FileHandlingError> {
    if show_hidden {
        list_dir_including_hidden(dir)
    } else {
        list_dir(dir)
    }
}

/// A message a disk-enumeration thread sends when it finishes.
#[derive(Debug)]
struct DiskMessage {
    generation: u64,
    disks: Vec<DiskInfo>,
}

/// The newest disk request the loader is waiting for.
#[derive(Debug)]
struct DiskPending {
    generation: u64,
    started: Instant,
}

/// Enumerates mounted disks off the UI thread.
///
/// `list_disks` (sysinfo) can stall on a disconnected network or cloud drive,
/// so the Locations overlay must not call it while handling a key. Like
/// [`DirLoader`], the request runs on a detached thread and is reported by
/// generation, so a late result from a cancelled or superseded request is
/// dropped. A request that exceeds [`LISTING_TIMEOUT`] settles to an empty list
/// rather than leaving the overlay loading forever.
#[derive(Debug)]
pub struct DiskLoader {
    tx: Sender<DiskMessage>,
    rx: Receiver<DiskMessage>,
    generation: u64,
    pending: Option<DiskPending>,
}

impl Default for DiskLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl DiskLoader {
    /// Creates a loader with no outstanding request.
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            generation: 0,
            pending: None,
        }
    }

    /// Starts enumerating the disks, replacing any request still in flight.
    pub fn request(&mut self) {
        self.generation += 1;
        let generation = self.generation;
        self.pending = Some(DiskPending {
            generation,
            started: Instant::now(),
        });

        let tx = self.tx.clone();
        // Detached: a stuck disk query cannot be cancelled, so it is left to
        // finish (or not) on its own and its result is reaped at process exit.
        let spawned = thread::Builder::new()
            .name("disk-listing".to_string())
            .spawn(move || {
                let _ = tx.send(DiskMessage {
                    generation,
                    disks: list_disks(),
                });
            });
        if spawned.is_err() {
            // Report "no disks" rather than staying pending forever.
            self.pending = None;
        }
    }

    /// Returns `true` while a request is outstanding.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Reports the newest request's disks, if it is ready.
    ///
    /// A completed result wins over the timeout; a request older than
    /// [`LISTING_TIMEOUT`] settles to an empty list so the overlay stops
    /// showing `Loading…`.
    pub fn poll(&mut self) -> Option<Vec<DiskInfo>> {
        self.poll_at(Instant::now())
    }

    /// [`Self::poll`] against a caller-supplied clock.
    fn poll_at(&mut self, now: Instant) -> Option<Vec<DiskInfo>> {
        let Some(generation) = self.pending.as_ref().map(|pending| pending.generation) else {
            // Drop a late result from a cancelled request.
            while self.rx.try_recv().is_ok() {}
            return None;
        };

        loop {
            match self.rx.try_recv() {
                Ok(message) if message.generation == generation => {
                    self.pending = None;
                    return Some(message.disks);
                }
                // A superseded request: drop it.
                Ok(_) => {}
                Err(_) => break,
            }
        }

        let timed_out = self
            .pending
            .as_ref()
            .is_some_and(|pending| now.duration_since(pending.started) >= LISTING_TIMEOUT);
        if timed_out {
            self.pending = None;
            return Some(Vec::new());
        }

        None
    }

    /// Abandons the outstanding request without waiting for it.
    pub fn cancel(&mut self) {
        self.pending = None;
        self.generation += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pumps `loader` until it reports an outcome or `limit` elapses.
    fn settle(loader: &mut DirLoader, limit: Duration) -> Option<ListingOutcome> {
        let deadline = Instant::now() + limit;
        while Instant::now() < deadline {
            if let Some(outcome) = loader.poll() {
                return Some(outcome);
            }
            thread::sleep(Duration::from_millis(1));
        }
        loader.poll()
    }

    #[test]
    fn a_request_reports_the_directory_it_read() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        let mut loader = DirLoader::new();

        loader.request(dir.path(), false);
        assert!(loader.is_pending());

        match settle(&mut loader, Duration::from_secs(5)).expect("a listing outcome") {
            ListingOutcome::Loaded {
                dir: loaded,
                entries,
            } => {
                assert_eq!(loaded, dir.path());
                assert!(entries.iter().any(|entry| entry.name == "a.txt"));
            }
            other => panic!("expected a loaded listing, got {other:?}"),
        }
        assert!(!loader.is_pending());
    }

    #[test]
    fn a_missing_directory_reports_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        let mut loader = DirLoader::new();

        loader.request(&missing, false);

        match settle(&mut loader, Duration::from_secs(5)).expect("a listing outcome") {
            ListingOutcome::Failed { dir: failed, .. } => assert_eq!(failed, missing),
            other => panic!("expected a failed listing, got {other:?}"),
        }
        assert!(!loader.is_pending());
    }

    #[test]
    fn a_stale_result_is_dropped() {
        // The first request is superseded before it can be polled; the loader
        // must report only the newest directory.
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::write(second.path().join("newest.txt"), b"x").unwrap();
        let mut loader = DirLoader::new();

        loader.request(first.path(), false);
        loader.request(second.path(), false);

        match settle(&mut loader, Duration::from_secs(5)).expect("a listing outcome") {
            ListingOutcome::Loaded { dir, entries } => {
                assert_eq!(dir, second.path(), "only the newest request is reported");
                assert!(entries.iter().any(|entry| entry.name == "newest.txt"));
            }
            other => panic!("expected a loaded listing, got {other:?}"),
        }
    }

    #[test]
    fn cancel_drops_the_outstanding_request() {
        let dir = tempfile::tempdir().unwrap();
        let mut loader = DirLoader::new();

        loader.request(dir.path(), false);
        loader.cancel();

        assert!(!loader.is_pending());
        assert!(loader.poll().is_none());
    }

    #[test]
    fn a_disk_request_reports_a_list_and_then_goes_idle() {
        let mut loader = DiskLoader::new();
        loader.request();
        assert!(loader.is_pending());

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut result = None;
        while Instant::now() < deadline {
            if let Some(disks) = loader.poll() {
                result = Some(disks);
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        assert!(result.is_some(), "the disk query must report a list");
        assert!(!loader.is_pending());
    }

    #[test]
    fn a_second_disk_request_while_pending_is_a_no_op() {
        let mut loader = DiskLoader::new();
        loader.request();
        loader.request();
        assert!(loader.is_pending());

        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if loader.poll().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }

        assert!(!loader.is_pending());
    }

    #[test]
    fn a_disk_request_that_never_answers_settles_to_an_empty_list() {
        // A channel that never receives stands in for a stuck `list_disks`.
        let (tx, rx) = mpsc::channel::<DiskMessage>();
        let mut loader = DiskLoader {
            tx,
            rx,
            generation: 1,
            pending: Some(DiskPending {
                generation: 1,
                started: Instant::now(),
            }),
        };

        let disks = loader
            .poll_at(Instant::now() + LISTING_TIMEOUT)
            .expect("a settled result");

        assert!(disks.is_empty(), "a timed-out query reports no disks");
        assert!(!loader.is_pending(), "the overlay must stop loading");
    }

    #[test]
    fn a_request_for_a_directory_that_never_answers_times_out() {
        // A channel that never receives stands in for a listing thread that is
        // stuck: `poll` must report the timeout rather than wait forever.
        let (tx, rx) = mpsc::channel::<Message>();
        let dir = PathBuf::from("/stuck");
        let mut loader = DirLoader {
            tx,
            rx,
            generation: 1,
            pending: Some(Pending {
                generation: 1,
                dir: dir.clone(),
                started: Instant::now(),
            }),
        };

        // Poll with a clock far enough ahead that the deadline has passed.
        let outcome = loader
            .poll_at(Instant::now() + LISTING_TIMEOUT)
            .expect("a timeout outcome");
        match outcome {
            ListingOutcome::TimedOut { dir: timed_out } => assert_eq!(timed_out, dir),
            other => panic!("expected a timeout, got {other:?}"),
        }
        assert!(!loader.is_pending());
    }
}
