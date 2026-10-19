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

Tokio is used only for input: `crossterm::event::EventStream` feeds a
`tokio::select!` over `run_app` on a current-thread runtime. The writer itself
stays a plain `std::thread` and uses `Mutex` + `Condvar`, so no async runtime is
required for blocking filesystem and cryptography work.

**Rejected alternatives:**

- **Synchronous save (previous behavior).** Correct and simple, but every
  commit blocks the UI for the duration of the scrypt derivation.
- **A queue of snapshots.** Wastes a full scrypt derivation per queued edit and
  can write an old root after a newer one. A single latest-wins slot gives the
  same durability with at most one redundant save.
- **`tokio::task::spawn_blocking` per save.** Concurrent saves race on the
  shared `{path}.tmp`, `{path}.bak` and rename sequence, and completion order is
  not submission order. One serialized writer avoids both problems.
- **A full async filesystem/encryption stack.** age and the atomic-save path are
  blocking APIs; wrapping them in `spawn_blocking` would add complexity without
  changing the serialization requirement. Only input genuinely benefits from
  async.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `src/file_handling/autosave.rs` | `Autosave` writer thread, latest-wins slot, generation counters, `SaveEvent`, panic containment |
| `src/tree/app_tree.rs` | `EventStream` + `tokio::select!` loop, submit-on-commit, status updates, exit flush, save-error overlay |

## Writer contract

- `Autosave::spawn(save, on_event)` starts one named thread (`vault-save`).
- `Autosave::submit(root) -> u64` clones the root, replaces any waiting job, and
  returns a monotonically increasing generation.
- The worker takes the slot, calls `save(&root)`, and sends
  `SaveEvent { generation, result }`. A panicking `save` is caught and reported
  as `SaveError::Panicked`, so the thread survives.
- `Drop` sets the shutdown flag, notifies the worker, and joins it. A snapshot
  still waiting in the slot is written before the thread exits.
- Notifications use `tokio::sync::mpsc::unbounded_channel`. There is exactly one
  producer (the save thread) and one consumer (the event loop), and the slowest
  producer is one event per save, so unbounded buffering cannot grow.

## Event-loop contract

- `TreeState.modified` means "changes not yet submitted to the worker". It is
  cleared immediately after `submit`, not after the write completes.
- A save event updates the status only when its generation equals `submitted`;
  events for superseded generations are ignored for display but still advance
  `saved`.
- `Saving...` is shown while the latest submission is outstanding.
  `Save failed` is shown in the title bar when the latest attempt fails; the full
  error is stored in `TreeState.save_error` and drawn as an overlay. Any key
  dismisses the overlay without touching the current `InputMode` or form state.
- On `q` or `H` the loop first retries the final snapshot if the last attempt
  failed or `modified` is set, then waits for the latest generation. On failure
  the quit flags are cleared, the error overlay is shown, and the user stays in
  the Tree. On success the function returns, and the caller copies the final
  root back to the home screen's `ActiveVaultState.node`.
- The master passphrase is moved into the worker closure; `TreeState` no longer
  stores it. Vaults without a path (the library demo) run without a worker.

## Home-screen boundary

The home screen keeps its blocking `event::read()` loop. Only the tree uses
`EventStream`, and the two loops never run concurrently: switching screens
replaces one with the other. Explicit home actions (import, change password,
create) still save synchronously.

## Consequences

- Input remains responsive while a save is in flight.
- At most one save per edit burst is duplicated, and the newest root always
  wins.
- Changes already submitted are flushed on `q`/`H`. A hard kill (SIGKILL,
  power loss) can lose the currently saving generation, as with any autosave.
- During the exit flush the loop stops consuming input; keys pressed in that
  window are buffered by crossterm and handled by the destination screen.
- Three copies of the tree can exist briefly: the live root, the submitted
  snapshot, and the worker's reference during saving.
