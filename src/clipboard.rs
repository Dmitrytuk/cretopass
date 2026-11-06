//! Gateway to the system clipboard.
//!
//! [`copy_text`] is the only place the app *writes* to the OS clipboard; a paste
//! read runs through [`PasteJob`], which moves the blocking read onto a worker
//! thread so a hung selection owner cannot freeze the UI. Both directions use
//! [`arboard`] on every platform (Windows included, via Win32 `CF_UNICODETEXT`,
//! so non-ASCII text survives), and fall back to the platform's command-line
//! helper only when arboard cannot provide one.
//!
//! Unlike the rest of the app's I/O it does not go through `file_handling`: the
//! clipboard is not a document. The gateway is nonetheless the one module that
//! owns the failure taxonomy ([`ClipboardError`]) and the process-wide handle.

use std::fmt;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Mutex, OnceLock, TryLockError};
use std::time::{Duration, Instant};

use zeroize::{Zeroize, Zeroizing};

/// How long the UI waits for a clipboard read before giving up on it.
///
/// A hung X11 selection owner or a wedged helper can block `get_text`/`output`
/// indefinitely. The worker is detached, so after this bound the UI stops
/// waiting and treats the paste as a silent no-op; the worker's late result (if
/// any) is discarded.
pub(crate) const PASTE_TIMEOUT: Duration = Duration::from_millis(500);

/// At most one clipboard read worker may run at a time.
///
/// A wedged read never returns; without this guard every `Ctrl+V` would spawn
/// another thread that blocks on the same cached handle. The flag is cleared by
/// the worker when it finally finishes, so a transiently slow read recovers.
static PASTE_READ_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Releases [`PASTE_READ_IN_FLIGHT`] when the worker finishes — including when
/// the reader panics, which a plain `store` after `read()` would miss and leave
/// paste permanently `Busy`.
struct InFlightGuard;

impl InFlightGuard {
    /// Claims the single-flight slot, or `None` when a worker already holds it.
    fn acquire() -> Option<Self> {
        (!PASTE_READ_IN_FLIGHT.swap(true, Ordering::AcqRel)).then_some(Self)
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        PASTE_READ_IN_FLIGHT.store(false, Ordering::Release);
    }
}

/// A clipboard read running on a worker thread.
///
/// `spawn` starts the read; the UI loop polls [`PasteJob::poll`] on its normal
/// tick and never blocks on the clipboard. The worker is deliberately detached:
/// if it never returns, its result is dropped and the UI simply moved on.
pub(crate) struct PasteJob {
    rx: Receiver<Result<Zeroizing<String>, ClipboardError>>,
    deadline: Instant,
}

impl PasteJob {
    /// Starts reading the system clipboard on a worker thread.
    ///
    /// # Errors
    /// Returns [`ClipboardError::Busy`] when a previous read is still running
    /// (a wedged clipboard), or [`ClipboardError::Spawn`] when the worker thread
    /// cannot be spawned.
    pub(crate) fn spawn() -> Result<Self, ClipboardError> {
        Self::spawn_with(PASTE_TIMEOUT, read_text)
    }

    /// The testable form of [`spawn`](Self::spawn): an injected reader and an
    /// explicit timeout.
    fn spawn_with(
        timeout: Duration,
        read: impl FnOnce() -> Result<Zeroizing<String>, ClipboardError> + Send + 'static,
    ) -> Result<Self, ClipboardError> {
        let Some(guard) = InFlightGuard::acquire() else {
            return Err(ClipboardError::Busy);
        };

        let (tx, rx) = mpsc::channel();
        // The guard moves into the worker and is released when the closure
        // returns or unwinds. If `spawn` fails, the closure (and the guard) are
        // dropped right here.
        let spawned = std::thread::Builder::new()
            .name("clipboard-paste".to_string())
            .spawn(move || {
                let _guard = guard;
                let result = read();
                let _ = tx.send(result);
            });

        if let Err(error) = spawned {
            return Err(ClipboardError::Spawn(error.to_string()));
        }

        Ok(Self {
            rx,
            deadline: Instant::now() + timeout,
        })
    }

    /// Returns the read's outcome once it is ready or the timeout elapses.
    ///
    /// `None` means the worker is still running; poll again after the next tick.
    pub(crate) fn poll(&mut self) -> Option<Result<Zeroizing<String>, ClipboardError>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Disconnected) => Some(Err(ClipboardError::WorkerStopped)),
            Err(TryRecvError::Empty) if Instant::now() >= self.deadline => {
                Some(Err(ClipboardError::Timeout))
            }
            Err(TryRecvError::Empty) => None,
        }
    }
}

/// The one cached clipboard handle for the process.
///
/// On X11 and Wayland the clipboard contents are served by the owning process,
/// so the handle must outlive the write: dropping it right after `set_text`
/// makes the value vanish. Caching it here keeps the value pasteable while the
/// app runs. On Windows and macOS the handle is cheap and the cache is harmless.
static CELL: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();

/// Copies `text` to the system clipboard.
///
/// Tries arboard first and the platform helper ladder second. Success keeps the
/// arboard handle cached; an initialisation failure leaves it uncached (the next
/// call retries) and a write failure drops it (the next call re-initialises).
///
/// # Errors
/// Returns [`ClipboardError`] when neither arboard nor a helper could take the
/// value. A helper is only tried when arboard fails, and only on Linux/macOS.
pub(crate) fn copy_text(text: &str) -> Result<(), ClipboardError> {
    match copy_with_arboard(text) {
        Ok(()) => Ok(()),
        // arboard failed: fall back to the platform helper. If no helper takes
        // it, report arboard's error, which describes the real problem.
        Err(error) => {
            if copy_with_helper(text) {
                Ok(())
            } else {
                Err(error)
            }
        }
    }
}

/// Copies `text` through the cached [`arboard::Clipboard`].
///
/// Opens the handle on first use and keeps it on success. A poisoned mutex is
/// recovered from: the cached handle carries no invariant that a panic could
/// have broken, so refusing to use it would only break copying for good.
///
/// Never blocks on the handle: an in-flight paste worker may hold it while a
/// wedged selection owner hangs, and this runs on the UI thread. If the lock is
/// taken, [`ClipboardError::Busy`] lets [`copy_text`] fall back to the helper.
fn copy_with_arboard(text: &str) -> Result<(), ClipboardError> {
    let cell = CELL.get_or_init(|| Mutex::new(None));
    let mut guard = match cell.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(TryLockError::WouldBlock) => return Err(ClipboardError::Busy),
    };

    if guard.is_none() {
        match arboard::Clipboard::new() {
            Ok(clipboard) => *guard = Some(clipboard),
            Err(error) => return Err(ClipboardError::Init(error.to_string())),
        }
    }

    let result = guard
        .as_mut()
        .expect("a handle was just installed")
        .set_text(text);
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            // The handle may be wedged (the display went away, the channel
            // broke): drop it so the next copy opens a fresh one.
            *guard = None;
            Err(ClipboardError::Write(error.to_string()))
        }
    }
}

/// Copies `text` through the platform's helper command, if one exists.
///
/// Returns `false` when no helper is configured for the platform or every
/// helper failed. A helper only counts as success when it exits zero: a
/// present-but-broken helper (a headless `xclip` that cannot open the display)
/// must not masquerade as a copy, so the ladder falls through to the next one.
fn copy_with_helper(text: &str) -> bool {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "-b"],
        ]
    } else {
        return false;
    };

    for args in cmds {
        if let Ok(mut child) = std::process::Command::new(args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            // Take the pipe out of the child and let it drop at the end of the
            // block: the helper (xclip/xsel) reads stdin to EOF before it takes
            // the selection, so the write end must be closed before `wait`.
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            if child.wait().is_ok_and(|status| status.success()) {
                return true;
            }
        }
    }
    false
}

/// Reads the system clipboard as text.
///
/// **Blocking:** called only from the [`PasteJob`] worker, never on the UI
/// thread. The value is wrapped in a [`Zeroizing`] buffer: it usually lands in a
/// password field, so it must be wiped from the heap when the caller is done.
///
/// Tries arboard first and the platform helper ladder second. An empty
/// clipboard is a successful read of the empty string, which a paste policy
/// then turns into a no-op.
///
/// # Errors
/// Returns [`ClipboardError`] when neither arboard nor a helper could provide
/// the text. A helper is only tried when arboard fails, and only on
/// Linux/macOS.
fn read_text() -> Result<Zeroizing<String>, ClipboardError> {
    match paste_with_arboard() {
        Ok(text) => Ok(text),
        // arboard failed to read: fall back to the platform helper. If no helper
        // provides text, report arboard's error, which describes the real cause.
        Err(error) => match paste_with_helper() {
            Some(text) => Ok(text),
            None => Err(error),
        },
    }
}

/// Reads the clipboard through the cached [`arboard::Clipboard`].
///
/// A read failure leaves the cached handle in place, unlike a failed write: on
/// X11 the handle serves the value this process last copied, so dropping it
/// would silently destroy it.
fn paste_with_arboard() -> Result<Zeroizing<String>, ClipboardError> {
    let cell = CELL.get_or_init(|| Mutex::new(None));
    let mut guard = match cell.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if guard.is_none() {
        match arboard::Clipboard::new() {
            Ok(clipboard) => *guard = Some(clipboard),
            Err(error) => return Err(ClipboardError::Init(error.to_string())),
        }
    }

    guard
        .as_mut()
        .expect("a handle was just installed")
        .get_text()
        .map(Zeroizing::new)
        .map_err(|error| ClipboardError::Read(error.to_string()))
}

/// Reads the clipboard through the platform's helper command, if one exists.
///
/// Returns `None` when no helper is configured for the platform or every helper
/// failed. A helper only counts as a read when it exits zero *and* its output is
/// valid UTF-8: a present-but-broken helper (a headless `xclip` that cannot open
/// the display) must not masquerade as an empty clipboard, so the ladder falls
/// through to the next one.
fn paste_with_helper() -> Option<Zeroizing<String>> {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbpaste"]]
    } else if cfg!(target_os = "linux") {
        &[
            &["wl-paste", "--no-newline"],
            &["xclip", "-o", "-selection", "clipboard"],
            &["xsel", "-b", "-o"],
        ]
    } else {
        return None;
    };

    for args in cmds {
        let Ok(output) = std::process::Command::new(args[0])
            .args(&args[1..])
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        match String::from_utf8(output.stdout) {
            Ok(text) => return Some(Zeroizing::new(text)),
            // Not usable text: wipe the bytes we read before trying the next
            // helper, so raw clipboard contents do not linger.
            Err(error) => {
                let mut bytes = error.into_bytes();
                bytes.zeroize();
            }
        }
    }
    None
}

/// Why a clipboard operation failed.
///
/// The only project-owned clipboard error, so a caller can render it without
/// naming `arboard`. The reason strings are arboard's already-rendered messages;
/// they describe the environment (no display, no server), not the text.
#[derive(Debug)]
pub(crate) enum ClipboardError {
    /// The clipboard could not be opened at all (no display or server).
    Init(String),
    /// The clipboard was opened but the value could not be written.
    Write(String),
    /// The clipboard was opened but the value could not be read.
    Read(String),
    /// A previous read is still running (the clipboard is wedged), so no new
    /// worker was started.
    Busy,
    /// The read did not finish within [`PASTE_TIMEOUT`].
    Timeout,
    /// The worker thread disappeared before reporting a result.
    WorkerStopped,
    /// The worker thread could not be started.
    Spawn(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Init(reason) => write!(f, "cannot open the system clipboard: {reason}"),
            Self::Write(reason) => write!(f, "cannot write to the system clipboard: {reason}"),
            Self::Read(reason) => write!(f, "cannot read the system clipboard: {reason}"),
            Self::Busy => f.write_str("the system clipboard is busy"),
            Self::Timeout => f.write_str("the clipboard read timed out"),
            Self::WorkerStopped => f.write_str("the clipboard read worker stopped"),
            Self::Spawn(reason) => write!(f, "cannot start the clipboard read worker: {reason}"),
        }
    }
}

impl std::error::Error for ClipboardError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Serialises the job tests: `PASTE_READ_IN_FLIGHT` is process-wide.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn test_lock() -> std::sync::MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Waits until the previous worker has released the process-wide guard.
    fn wait_until_idle() {
        let deadline = Instant::now() + Duration::from_secs(2);
        while PASTE_READ_IN_FLIGHT.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!PASTE_READ_IN_FLIGHT.load(Ordering::Acquire));
    }

    /// Polls `job` until it reports, with a generous deadline so a slow CI box
    /// cannot turn this into a flaky timeout.
    fn poll_until_ready(job: &mut PasteJob) -> Result<Zeroizing<String>, ClipboardError> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = job.poll() {
                return result;
            }
            assert!(Instant::now() < deadline, "the worker never reported");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_paste_job_reports_a_successful_read() {
        let _guard = test_lock();
        wait_until_idle();

        let mut job = PasteJob::spawn_with(PASTE_TIMEOUT, || Ok(Zeroizing::new("secret".into())))
            .expect("the worker starts");
        let result = poll_until_ready(&mut job);

        assert_eq!(result.expect("the read succeeds").as_str(), "secret");
        wait_until_idle();
    }

    #[test]
    fn a_paste_job_reports_a_failed_read() {
        let _guard = test_lock();
        wait_until_idle();

        let mut job = PasteJob::spawn_with(PASTE_TIMEOUT, || {
            Err(ClipboardError::Read("no display".to_string()))
        })
        .expect("the worker starts");
        let error = poll_until_ready(&mut job).expect_err("the read fails");

        assert!(matches!(error, ClipboardError::Read(_)));
        wait_until_idle();
    }

    #[test]
    fn a_paste_job_times_out_and_releases_the_guard() {
        let _guard = test_lock();
        wait_until_idle();

        let (released_tx, released_rx) = mpsc::channel();
        let mut job = PasteJob::spawn_with(Duration::from_millis(10), move || {
            std::thread::sleep(Duration::from_millis(40));
            let _ = released_tx.send(());
            Ok(Zeroizing::new("late".to_string()))
        })
        .expect("the worker starts");

        let error = poll_until_ready(&mut job).expect_err("the read times out");
        assert!(matches!(error, ClipboardError::Timeout));
        drop(job); // the UI stops waiting

        // The worker still finishes; it must release the guard for the next paste.
        released_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the worker finishes");
        wait_until_idle();
    }

    #[test]
    fn a_second_paste_is_rejected_while_the_first_runs() {
        let _guard = test_lock();
        wait_until_idle();

        let (release_tx, release_rx) = mpsc::channel();
        let mut first = PasteJob::spawn_with(PASTE_TIMEOUT, move || {
            let _ = release_rx.recv();
            Ok(Zeroizing::new("first".to_string()))
        })
        .expect("the first worker starts");

        let second = PasteJob::spawn_with(PASTE_TIMEOUT, || Ok(Zeroizing::new("second".into())));
        assert!(matches!(second, Err(ClipboardError::Busy)));

        release_tx.send(()).expect("the reader is still listening");
        let result = poll_until_ready(&mut first);
        assert_eq!(result.expect("the first read succeeds").as_str(), "first");
        wait_until_idle();
    }

    #[test]
    fn an_init_error_names_the_failed_step() {
        assert_eq!(
            ClipboardError::Init("no display".to_string()).to_string(),
            "cannot open the system clipboard: no display"
        );
    }

    #[test]
    fn a_write_error_names_the_failed_step() {
        assert_eq!(
            ClipboardError::Write("broken pipe".to_string()).to_string(),
            "cannot write to the system clipboard: broken pipe"
        );
    }

    #[test]
    fn a_read_error_names_the_failed_step() {
        assert_eq!(
            ClipboardError::Read("no owner".to_string()).to_string(),
            "cannot read the system clipboard: no owner"
        );
    }

    #[test]
    fn the_worker_failures_name_themselves() {
        assert_eq!(
            ClipboardError::Busy.to_string(),
            "the system clipboard is busy"
        );
        assert_eq!(
            ClipboardError::Timeout.to_string(),
            "the clipboard read timed out"
        );
        assert_eq!(
            ClipboardError::WorkerStopped.to_string(),
            "the clipboard read worker stopped"
        );
        assert_eq!(
            ClipboardError::Spawn("no threads".to_string()).to_string(),
            "cannot start the clipboard read worker: no threads"
        );
    }

    #[test]
    fn the_in_flight_guard_is_released_on_drop() {
        let _guard = test_lock();
        wait_until_idle();

        {
            let held = InFlightGuard::acquire().expect("the slot is free");
            assert!(
                InFlightGuard::acquire().is_none(),
                "only one worker may run at a time"
            );
            drop(held);
        }

        assert!(
            InFlightGuard::acquire().is_some(),
            "dropping the guard releases the slot"
        );
        wait_until_idle();
    }
}
