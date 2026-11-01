# 0010 — Asynchronous tree autosave

Saving a vault is expensive: age derives its key with scrypt at a work factor
tuned to about one second, and every save also rewrites the whole JSON file
through the atomic temp-file + fsync + rename path. Doing that work on the event
loop froze input for roughly a second after every mutation.

**Decision:** committed tree changes are written by one background writer
thread. The event loop submits a snapshot of the root and continues
immediately. A submitted snapshot replaces one that has not started yet
("latest wins"), because a newer snapshot always contains every earlier change.
The UI keeps two generation counters, `submitted` and `saved`, and a save
notification carries the generation it attempted.

Every wait on the writer is bounded: the exit flush and the worker join each
give up after `SAVE_TIMEOUT` (10 s) so a stalled filesystem cannot hang the UI or
process termination. Because a failed final flush must never trap the user in the
Tree, exits have an explicit retry / quit-without-saving escape hatch.

Tokio is used only for input and the bounded flush: `crossterm::event::EventStream`
feeds a `tokio::select!` over `run_app` on a current-thread runtime with its time
driver enabled. The writer itself stays a plain `std::thread` and uses `Mutex` +
`Condvar`, so no async runtime is required for blocking filesystem and
cryptography work.

**Rejected alternatives:**

- **Synchronous save (previous behavior).** Correct and simple, but every
  commit blocks the UI for the duration of the scrypt derivation.
- **A queue of snapshots.** Wastes a full scrypt derivation per queued edit and
  can write an old root after a newer one. A single latest-wins slot gives the
  same durability with at most one redundant save.
- **`tokio::task::spawn_blocking` per save.** Completion order is not
  submission order, so a newer snapshot could be overwritten by an older one.
  One serialized writer avoids that; the exclusive random temp name in
  `save_tree` also keeps concurrent writers from corrupting each other.
- **A full async filesystem/encryption stack.** age and the atomic-save path are
  blocking APIs; wrapping them in `spawn_blocking` would add complexity without
  changing the serialization requirement. Only input genuinely benefits from
  async.
- **Reporting a failed exit as success.** "Never lose changes" must not mean
  "never let the user leave": a force-quit the user explicitly confirms reports
  `ExitStatus::Unsaved` instead of pretending the vault was written.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `src/file_handling/autosave.rs` | generic `Autosave<T>` writer thread, latest-wins slot, generation counters, typed `SaveError`, `SaveEvent`, panic containment, bounded `shutdown`/`detach` |
| `src/tree/app_tree.rs` | `EventStream` + `tokio::select!` loop, submit-on-commit, `SaveState` updates, bounded exit flush, force-quit escape hatch, `SaveTarget` |

## Writer contract

- `Autosave::spawn(save, on_event)` starts one named thread (`vault-save`) and
  returns `Autosave<T>`, generic over the opaque snapshot type `T`.
- `Autosave::submit(root) -> u64` takes ownership of the snapshot, replaces any
  waiting job, and returns a monotonically increasing generation. The event loop
  clones the live root once per commit; this full-tree clone is deliberate and
  cheaper than another scrypt derivation. The replaced snapshot is dropped
  outside the mutex so releasing a large tree cannot block the worker.
- The worker takes the slot, calls `save(&root)`, and sends
  `SaveEvent { generation, result }`.
- `SaveError` is typed and `#[non_exhaustive]`: `Failed(FileHandlingError)`,
  `Panicked`, `WorkerStopped`. `Display` and `std::error::Error::source()`
  forward to the file error, so callers can distinguish categories without
  parsing a string.
- A panicking `save` is caught and reported as `SaveError::Panicked`, so the
  thread survives. A panicking `on_event` stops the worker and drops the sender;
  the app observes `save_rx.recv() -> None` and reports `WorkerStopped`. The
  default panic hook is left untouched, so a caught panic still prints a trace to
  stderr over the alternate screen.
- Lifecycle is explicit. `Autosave::shutdown(timeout)` stops the worker and
  returns `Err(SaveError::WorkerStopped)` **only** on a join timeout; a worker
  panic is surfaced through the closed channel, not here. `Autosave::detach()`
  stops without waiting, for a force-quit while a save is stuck. `Drop` is a
  best-effort bounded join (also `SAVE_TIMEOUT`) for callers that forget. On
  timeout or detach the thread is left to be reaped at process exit.
- A snapshot still waiting in the slot is written before the thread exits.
- The superseded snapshot is wiped. `submit` zeroizes a replaced still-waiting
  snapshot, and the worker zeroizes each snapshot after `save` returns — on
  success, failure, or a caught panic — so plaintext does not outlive its job.
- Cancelling is separate from stopping the worker. `Autosave` owns a
  `file_handling::CancelToken` and hands a clone to the save function, which
  checks it immediately **before** the rename
  (`atomic::write_atomic_cancellable`). `detach()`, and a `shutdown(timeout)` or
  `Drop` join that times out, cancel the token, so a writer stalled across a
  force-quit cannot replace a newer file. A cancelled write reports
  `FileHandlingError::WriteAbandoned` and leaves the target byte-identical; the
  only residual window is the rename itself.
- Notifications use `tokio::sync::mpsc::unbounded_channel`. There is exactly one
  producer (the save thread) and one consumer (the event loop), and the slowest
  producer is one event per save, so unbounded buffering cannot grow.

## Save target

- `run_app(terminal, state, target: Option<SaveTarget>)` receives both the path
  and the passphrase in one `SaveTarget { path, passphrase }`, so "path present,
  passphrase absent" is unrepresentable and no longer aborts the session with an
  I/O error. `None` means a read-only tree (the library demo) with no worker.
- `SaveTarget` has a hand-written `Debug` that redacts the passphrase.

## Event-loop contract

- `TreeState.modified` means "changes not yet submitted to the worker". It is
  cleared immediately after `submit`, not after the write completes.
- `TreeState.save_state: SaveState { Idle, Saving, Failed { error, acknowledged } }`
  is the single source of truth for "saving" vs "failed"; `title_status` and the
  save-error overlay both derive from it, and no control flow compares UI strings.
- A save event updates `save_state` only when its generation equals `submitted`;
  events for superseded generations are ignored for display but still advance
  `saved`.
- The title bar shows `Saving` (amber) with an animated glyph while the latest
  submission is outstanding, and `Save failed` (red) when the latest attempt
  fails. `SaveState::title_status` maps the state to a `theming::Status`; an
  interval tick advances the glyph only while a save is outstanding (see
  [0011](0011-animated-status-spinner.md)). An unacknowledged failure draws the
  overlay with the full error. Any key acknowledges it without touching the
  current `InputMode` or form state.
- On `q`, `H`, or `Ctrl-C` the loop decides via the pure `exit_plan` helper: if
  `modified` is set or the last attempt failed it re-submits, otherwise it waits
  for the outstanding generation. The flush is bounded by
  `tokio::time::timeout(SAVE_TIMEOUT, …)`.
- On flush failure (or timeout) the exit is **parked** in
  `TreeState.save_exit = SaveExit::Blocked` and the overlay offers actions:
  `r`/`Enter` retry the flush, `q`/`Ctrl-C` request a confirmed quit, any other
  key cancels the exit and stays. `SaveExit::ConfirmDiscard` shows
  `Unsaved changes could not be saved. Quit anyway? [y/N]`; `y` leaves, anything
  else cancels. While parked the flush branch does not re-run, so "dismiss and
  stay" cannot be re-triggered by the next key.
- `run_app` returns `ExitStatus::Saved` when the latest snapshot is durable (or
  there is no worker) and `ExitStatus::Unsaved` when the user force-quit.
- The master passphrase is moved into the worker closure; `TreeState` no longer
  stores it.

## Home-screen boundary

The home screen keeps its blocking `event::read()` loop. Only the tree uses
`EventStream`, and the two loops never run concurrently: switching screens
replaces one with the other. Explicit home actions (import, change password,
create) run all of their blocking work on a `std::thread` — the import's file
read and parse (under `Status::Loading`) as well as the scrypt — while the UI
thread redraws the animated status at `SPINNER_TICK` and discards input until
the work finishes (see [0011](0011-animated-status-spinner.md)).

`run_tree` maps the tree result back onto `HomeState`:

- `ExitStatus::Saved`: copy the final root into `ActiveVaultState.node`, so later
  home operations do not overwrite the worker's edits with stale data.
- `ExitStatus::Unsaved`: drop the active vault entirely, so no later home action
  can persist or overwrite with the discarded in-memory tree.
- An I/O error: still sync the final root (best effort) before propagating.

## Consequences

- Input remains responsive while a save is in flight.
- At most one save per edit burst is duplicated, and the newest root always
  wins.
- Changes already submitted are flushed on `q`/`H`. A hard kill (SIGKILL,
  power loss) can lose the currently saving generation, as with any autosave.
- A stalled save can no longer hang the UI or process exit: the flush returns
  after `SAVE_TIMEOUT`, and a force-quit detaches the stuck worker.
- A force-quit never reports the vault as saved; Home drops the active vault so
  the on-disk copy is not overwritten with discarded edits.
- During the exit flush the loop stops consuming input; keys pressed in that
  window are buffered by crossterm and handled by the destination screen.
- A detached worker keeps running until process exit; its final write may or may
  not complete, which is exactly the uncertainty the user accepted by confirming.
- Three copies of the tree can exist briefly: the live root, the submitted
  snapshot, and the worker's reference during saving.
