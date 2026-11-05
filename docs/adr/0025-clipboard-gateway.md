# 0025 — Clipboard gateway

The Detail modal copies an extra field's value to the OS clipboard. On Windows
the old implementation was a no-op: `copy_to_clipboard` picked its command ladder
with `cfg!(target_os = "macos")` / `cfg!(target_os = "linux")`, both false on
Windows, so it returned immediately — and Detail had already set its
unconditional `✓ Copied!` alert *before* asking, so the user saw success with an
empty clipboard. The same false success appeared on a Linux box with none of
`wl-copy`/`xclip`/`xsel` in `PATH`.

**Decision:** one gateway, `clipboard::copy_text`, is the only place a secret
reaches the OS clipboard. It uses `arboard` on every platform and reports an
honest success or failure; the platform command-line helpers survive only as a
fallback behind arboard.

Rules that follow from it:

- **arboard first, helpers second.** arboard reaches the native clipboard on all
  three platforms. Only when it fails, and only on Linux/macOS, is the old ladder
  tried (`wl-copy` → `xclip -selection clipboard` → `xsel -b`; `pbcopy` on
  macOS). The value always goes over the helper's **stdin**, never as an argument,
  so it cannot leak through a process listing. A helper counts as success only
  when it exits zero: a present-but-broken `xclip` (headless, no `DISPLAY`) falls
  through to the next helper and finally to arboard's error.
- **Windows goes through `CF_UNICODETEXT`.** arboard's Win32 backend writes a
  UTF-16 clipboard item, so Cyrillic and other non-ASCII values survive. This is
  why `cmd /C clip` (console code page) was rejected.
- **One process-wide handle, kept alive.** A
  `static OnceLock<Mutex<Option<arboard::Clipboard>>>` caches the handle. On X11
  and Wayland the clipboard contents are served by the owning process while its
  handle lives, so dropping the handle right after `set_text` makes the value
  vanish; the cache keeps it pasteable for the rest of the app's run.
- **The failure kind drives the handle.** `ClipboardError::Init` leaves the slot
  empty so the next copy retries; `ClipboardError::Write` drops the handle so the
  next copy re-initialises. A poisoned mutex is recovered with
  `PoisonError::into_inner()`: the cache holds no invariant a panic could break,
  so refusing to use it would only break copying for good.
- **The verdict is a uniform result modal.** `Detail` no longer owns a transient
  alert: `CopyToClipboard` is handled in
  [`orchestrator`](../../src/tree/widget/orchestrator.rs) by `report_copy`, which
  installs `Modal::success(COPY_CHECK)` or
  `Modal::error(MSG_COPY_FAILED + error)`. The alert replaces the Detail modal,
  so a failure can never be mistaken for the old unconditional check mark.
- **An empty value is a silent no-op.** Detail emits no action for an empty
  field rather than writing it, which would wipe the user's clipboard.

## Dependency choices

`arboard = { version = "3.7", default-features = false }`.

- `default-features = false` drops the `image-data` feature and the `image`
  crate it pulls in; only text is needed. The macOS `objc2-core-graphics` crate
  still arrives unconditionally through arboard's `objc2-app-kit` dependency.
- The `wayland-data-control` feature is deliberately **not** enabled: it pulls in
  `wl-clipboard-rs` and a wayland stack (+13 crates) for a capability the fallback
  ladder already covers. Without it, arboard's Linux backend is X11 only — it
  ignores `WAYLAND_DISPLAY` — so a Wayland session with XWayland goes through X11,
  and a pure-Wayland session without XWayland and without `wl-copy` now yields an
  honest error instead of a silent no-op.

## Consequences

- A small tree of transitive crates enters the graph (`arboard`, `x11rb`,
  `x11rb-protocol`, `percent-encoding`, `gethostname`, the Windows
  `clipboard-win`/`error-code`, and the mac-only `objc2*`); all permissive,
  listed in `LICENSE-THIRD-PARTY`.
- A copy that cannot reach the clipboard is now visible: `ClipboardError` renders
  as `cannot open the system clipboard: …` or `cannot write to the system
  clipboard: …`, prefixed by `MSG_COPY_FAILED` in the alert.
- The platform helper ladder moves out of `tree::widget::orchestrator` into the
  gateway. Like `generator` and `share`, `clipboard` sits below `tree` and never
  imports it. Unlike the rest of the app's I/O it does not go through
  `file_handling` — the clipboard is not a document.

## Rejected alternatives

- *Windows-only `cmd /C clip`*: the console code page mangles non-ASCII values.
- *`clipboard-win` under `cfg(windows)` only*: a second platform-specific
  implementation to maintain around arboard, which already wraps it.
- *OSC 52 terminal escape*: depends on terminal support and routing, and some
  terminals refuse or truncate it; not a dependable copy.
- *Keep the helper-only path*: no Windows backend at all, and no way to tell a
  successful copy from a missing helper.
- *Drop the arboard handle right after each write*: breaks paste on X11 and
  Wayland, where the process serves the selection.
