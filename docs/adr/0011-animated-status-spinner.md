# 0011 — Animated status spinner

The title bar used to show a static `Loading...` / `Saving...` while a blocking
operation ran. In a debug build `decrypt_to_node` takes about 1.4 s (age's scrypt
is deliberately expensive) and saves are comparable, so the label sat frozen for
the whole wait — indistinguishable from a hang. A glyph that keeps moving for the
entire duration makes "working" visibly different from "stuck".

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Animate, not a static glyph.** An 8-frame braille spinner advances every `SPINNER_TICK` (80 ms, ≈12.5 fps) while a busy status is shown. | Motion is what distinguishes "working" from "frozen"; a static glyph carries no timing information. |
| 2 | **No new dependency.** `theming::spinner` holds the frame array and a `usize` index advanced with `% FRAMES.len()`. | `throbber-widgets-tui`'s `ThrobberState` stores a private `i8` advanced with `saturating_add`, so the index sticks at `127` after ~10 s (≈125 ticks at 80 ms) and `127 % 8 == 7` freezes the animation; it also pins `ratatui` to 0.28 (0.8+ needs 0.29/0.30). It would contribute nothing but 8 constants. A `usize` + modulo stays in range by construction — no saturation, no overflow, no negative index. |
| 3 | **One shared `theming::Status`.** `Loading` / `Saving` / `SaveFailed` with `label()`, `spins()`, `style()`. Home's `status` becomes `Option<Status>`; the Tree keeps `SaveState` and gains `title_status() -> Option<Status>` (replacing the `title_label()` / `title_style()` pair). | A single vocabulary for the title-bar status keeps the two screens' rendering identical and keeps string identity out of control flow. The Tree keeps `SaveState` because it also carries the `SaveError` and the `acknowledged` flag. |
| 4 | **Title format `" ⣷ Saving  File — path"`.** The glyph is shown only when the status spins; `Save failed` keeps a static label. Colors are unchanged: Loading green, Saving yellow, Save failed red+bold. | The glyph means "in progress"; a failure is not in progress. |
| 5 | **Home: work on a `std::thread`, UI thread busy-loops the redraw.** `run_with_spinner` spawns the closure, then `{ advance; draw; poll(SPINNER_TICK); drain }` until the worker finishes. | Home's outer loop is a blocking `event::read()`; the smallest change that animates is to move the one blocking call off-thread while the UI thread keeps drawing. A full async rework of Home is far larger for the same effect. |
| 6 | **Input during the Home wait is read and fully drained, then discarded.** | A stray Enter must not trigger a second action once the wait ends. Esc / Ctrl+C cannot cancel the scrypt work, so swallowing them is the honest behavior. |
| 7 | **Tree: a guarded `tokio::time::interval` tick.** `MissedTickBehavior::Delay`; the tick branch and the flush tick are both guarded by `save_state.spins()`. | `interval`'s first tick is immediate, but the guard is false while idle, so the app does not redraw at 12.5 fps when nothing is happening. The Tree draws once per 80 ms only while a save is outstanding. |
| 8 | **Tree: a blocking helper for the blocking writes.** `run_with_busy` mirrors Home's `run_with_spinner` for the Tree operations that block outside autosave — the share write and the export: a transient `operation_status` on `TreeState` takes title-bar precedence over `save_state`, and the helper advances the spinner and redraws until the worker finishes. | The share write (age scrypt + fsync, [0023](0023-sharing-a-vault-subtree.md)) and the export (full-tree serialisation plus file and directory `fsync`, [0022](0022-user-chosen-export-destination.md)) are as slow as a save and would otherwise freeze the UI. Mid-wait input is left to the `EventStream` rather than drained: polling crossterm would race its reader thread. |

## Home busy helper

`run_with_spinner<B, T, F>(terminal, state, status, work)`:

- sets `state.status` and draws once (matching the previous pre-call draw);
- spawns `work` with `std::thread::Builder`; a spawn failure is an `io::Error`,
  not a panic;
- loops until `handle.is_finished()`, advancing the spinner, redrawing, and
  polling/draining input each `SPINNER_TICK`;
- maps a worker panic to an `io::Error`;
- clears `state.status` and returns without a final draw — the call site sets the
  next modal and the outer loop's top-of-iteration draw renders the real result,
  avoiding a flash of the wrong screen.

The decrypt site moves the modal's owned `file_data` / `passphrase` into the
closure and returns them alongside the outcome, so the error path can rebuild the
passphrase prompt without a second copy of the secret.

Home's export uses the same helper with `Status::Saving`: serialising the whole
tree and `fsync`ing the file and its directory block, so they run on the worker
under the spinner, exactly as the Tree's share and export writes do.

**If `terminal.draw` fails mid-wait**, the worker is already running and the
save/decrypt is **not** cancelled. This is intended: the operation is
non-cancellable scrypt work, and abandoning it would risk a partially written
file. The `io::Error` propagates once the wait ends.

## Tree busy helper

`run_with_busy(terminal, state, status, draw, work)` mirrors the Home helper for
the Tree's blocking writes — the share write and the export:

- sets `state.operation_status` (a `TreeState` field the title bar prefers over
  `save_state`) and draws once;
- spawns `work` with `std::thread::Builder`;
- loops until the worker finishes, advancing the spinner and redrawing with the
  screen's full-frame renderer (`ui`), which the caller passes in;
- clears `operation_status` and returns, so the loop's next draw renders the
  result modal the caller installed.

Unlike Home, the Tree wait does **not** drain input: the Tree's keys arrive
through a tokio `EventStream` with its own reader thread, and polling crossterm
here would race it. Keys pressed during the wait are buffered and handled on the
next turn of the event loop.

## Rejected alternatives

- **`throbber-widgets-tui`.** See decision 2 — saturating index, `ratatui` 0.28
  pin, constants-only benefit.
- **Rework Home into an async `EventStream` loop like the Tree.** A much larger
  change (Home has many modal handlers and no runtime) for the same visual
  result; the tree's async loop already exists, which is why only it gets a tick.
- **Cancel the operation on Esc during the wait.** scrypt is not
  interruption-safe and a cancelled save could leave a partially written file.
  Discarding input keeps the fence honest.

## Consequences

- Resize during the Home wait needs no special handling: `terminal.draw` re-reads
  the backend size on every tick.
- The Home wait redraws without the passphrase modal, matching the previous
  behavior (the modal is taken out of `state.modal` before the blocking call).
- The Tree's idle event loop is unchanged: the guarded tick does not fire when no
  save is outstanding.
- The Tree title bar shows a blocking operation's status ahead of the autosave
  status; the two share one spinner frame counter.
