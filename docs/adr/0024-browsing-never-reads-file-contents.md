# 0024 — Browsing never reads file contents

On Windows, a cloud folder (Yandex Disk "cloud files", OneDrive Files
On-Demand, Dropbox online-only, Google Drive stream files) stores its files as
*placeholders*: the metadata is local, but the bytes live in the cloud.
`read_dir` and `stat` are cheap — they never trigger a download — but **opening
a placeholder for read hydrates it** (the provider downloads the whole file) and
Windows may raise a consent dialog asking whether `cretopass` may fetch it.

The File Browser's directory listing opened **every** regular file just to sniff
the 11-byte `age-encrypt` header, so browsing a synced folder downloaded the
whole folder and could pop a dialog hidden behind the full-screen TUI — observed
on Yandex Disk as an apparently permanent hang. A second, independent cause: all
directory I/O ran synchronously inside key handling (Home waits in a blocking
`event::read()`, the Tree in a `tokio::select!`), so any slow or blocked call —
a sync filter, a disconnected SMB/`\\wsl$` path, a hung FUSE mount — froze the
whole TUI with no spinner and no working `Esc`.

**Decision:** browsing reads no file contents, and every filesystem call behind
a key runs off the UI thread with a bounded wait.

Rules that follow from it:

- **Listing is name-only.** `file_handling::list_dir` / `list_dir_including_hidden`
  keep only the entry's name and `entry.file_type()`. A row's
  `FileEntry::looks_like_vault` field is a display hint computed from the name
  (case-insensitive `.age`) — no `is_age_file` / `scan_age_header` on the browsing
  path. The exact, content-based `is_age_file` survives only for callers that
  genuinely need the content (tests, and any future explicit probe); opening a
  vault (`validate_encrypted_file` / `decrypt_to_node`) and importing a document
  read contents because the user explicitly targeted that file, so hydration (and
  the provider's dialog) is expected there.
- **The filter is gone.** `FileFilter::PwdKeeperOnly` (the `f` toggle) could only
  work by knowing which files were vaults, which is exactly the content read this
  ADR removes; it is deleted from the app, its strings, and the docs.
- **`r` / `d` work on any non-directory row.** With no header check available,
  rename and delete are offered for any file row; the active vault stays
  protected (`MSG_CANNOT_DELETE_FILE`) and deletion still asks for confirmation.
- **The export overwrite guard is a name hint.** The File Browser and
  `export_tree` refuse a target whose name looks like a vault (`looks_like_vault`)
  and otherwise rely on the browser's overwrite confirmation. This replaces
  [ADR 0022](0022-user-chosen-export-destination.md)'s content-based
  `is_age_file` guard: recognising a secret by content is incompatible with
  hydration-free browsing. Because `output_path` always resolves the format's
  extension, the name-hint branch cannot fire through the browser today, so the
  overwrite confirmation is the real guard.
- **Directory listing runs on detached threads.** New
  `file_handling::listing::DirLoader` issues each request on its own thread,
  carrying a monotonic `generation`; a request's result is dropped when a newer
  request has superseded it, so fast navigation can never apply an older listing
  over a newer one. `LISTING_TIMEOUT` (10 s, mirroring `SAVE_TIMEOUT`) bounds the
  wait: a result that lands before the deadline wins, otherwise the request is
  abandoned and the browser restores the previous directory and listing (or
  falls back to Home for the first request) behind a dismissable notice.
- **A stuck thread is left to die.** Thread-per-request — not a coalescing worker
  like [`Autosave`](0010-async-tree-autosave.md) — is deliberate: a blocked fs call
  can be neither cancelled nor reused, so a worker would pin one thread forever.
  An abandoned thread is detached and reaped at process exit. The leak is bounded
  by user actions (one thread per stuck navigation) and is accepted; nothing is
  joined, because joining is the freeze this ADR exists to avoid.
- **The UI keeps drawing while it waits.** `FileBrowserState` only *requests*
  listings; `poll_listing` applies them and `is_loading` reports the wait. The
  list shows a `Loading…` row, `Esc` closes the browser immediately even while a
  call is stuck, and the Home and Tree loops redraw on the spinner tick while a
  listing is pending.
- **The Tree polls on the spinner tick, not a channel.** The Tree's
  `tokio::select!` does not gain a `listing_rx.recv()` branch: the loader's
  receiver lives inside the modal state, so a branch for it could not borrow
  independently of the `&mut TreeState` the event branch uses. The existing
  80 ms spinner tick (its guard extended with `browser_loading`) gives the same
  latency and is simpler.
- **Disk enumeration is backgrounded too.** The Locations overlay used
  `file_handling::list_disks` (sysinfo) synchronously, which stalls on a
  disconnected network or cloud drive. New `listing::DiskLoader` enumerates on a
  detached thread with the same generation and timeout rules; the overlay opens
  at once with the pinned Home row and a `Loading…` row, appends the disks when
  they arrive, and re-runs `preselect_location`; the query is cancelled when the
  overlay closes.
- **A delete that has not taken effect is explained.** If `delete_file` succeeds
  but the refreshed listing still contains the path — a Windows sync client still
  holds the handle — the browser shows a dismissable `MSG_DELETE_PENDING` notice
  instead of appearing to ignore the delete.

## Consequences

- [`FileBrowserState`](../../src/file_browser/docs/adr/0007-file-browser.md) is no
  longer `Clone` (it owns a `DirLoader`/`DiskLoader`) and its `entries` are empty
  until a listing lands; tests drive the async seam through `settle()`.
- A `*.age` file with foreign contents is marked as a vault, and a vault stored
  without the `.age` extension is not. Accepted: the hint is cosmetic, the marker
  is not a security boundary, and detection on opening is still content-based.
- The `f` filter and its `[f]` hint are gone from [ADR 0007](../../src/file_browser/docs/adr/0007-file-browser.md)
  and [ADR 0014](0014-status-bar-unification.md)'s `MODE_PWD` badge no longer
  exists.
- Key handling still performs cheap synchronous metadata calls (`exists`,
  `is_dir_empty`, `create_dir`, `delete_file`, `rename_file`); those do not
  hydrate placeholders and are out of scope here.
- On Windows, the app still cannot *see* that a file is cloud-only; it simply
  never forces a download. A future "cloud-only" badge can read the placeholder
  attribute without hydrating — Windows `MetadataExt::file_attributes`
  (`FILE_ATTRIBUTE_RECALL_ON_OPEN` `0x0004_0000`,
  `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` `0x0040_0000`,
  `FILE_ATTRIBUTE_OFFLINE` `0x0000_1000`) and macOS `MetadataExt::st_flags() &
  SF_DATALESS` (`0x4000_0000`) — which is deliberately left to separate work.

## Rejected alternatives

- *A coalescing worker (like `Autosave`) for listings*: a blocked `read_dir`
  cannot be cancelled or reused, so a single worker would pin one thread forever
  and serve no request. Thread-per-request with generation-based stale-drop is
  the only model that both supersedes results and tolerates a stuck call.
- *Blocking listing with a spinner (`run_with_spinner`)*: the spinner loops on
  the UI thread and cannot cancel the worker either; `Esc` would still be dead
  while a stuck call holds the thread.
- *Keep the content sniff and hydrate lazily*: there is no non-hydrating way to
  learn whether a cloud placeholder is an age vault, and any content read
  reintroduces the download and the hidden consent dialog.
- *Keep `f` but detect vaults by name only*: a filter that shows only `*.age`
  names is a weaker, surprising duplicate of the search box and still mislabels
  files; removed outright.
- *A dedicated Tree `listing_rx.recv()` branch*: it cannot share `&mut TreeState`
  with the event branch; the spinner tick already bounds latency to one tick.
