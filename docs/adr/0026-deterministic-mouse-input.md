# 0026 — Deterministic mouse input

The app used to leave the mouse entirely to the terminal: it never enabled mouse
capture, so the wheel and clicks were interpreted by each emulator, differently on
every OS. A wheel tick scrolled the terminal instead of the list, or did nothing;
text selection worked in one terminal and not another. Worse, `Ctrl+V` was unbound
and every input dispatcher matched `KeyCode::Char(c)` without checking modifiers, so
pressing it typed the letter `v` into the field.

**Decision:** the app takes the mouse and owns two deterministic behaviors — a wheel
tick is Up/Down navigation, and a click is hit-tested on a small on-screen region and
turned into the same `Action` its key would produce. Two new leaf modules carry it:

- **`terminal`** ([source](../../src/terminal.rs)) owns every terminal mode the app
  turns on, in one place: raw mode, the alternate screen, bracketed paste and mouse
  capture. `setup()` enables them; `restore()` undoes them, and it is idempotent, so
  it also serves the panic hook installed before any mode changes — teardown is
  symmetric on normal exit, on error and on panic. `lib.rs` stays thin wiring.
- **`hitbox`** ([source](../../src/hitbox.rs)) is a strict leaf over `ratatui` and
  `crossterm`, like `generator` and `clipboard`. `HitBox<T>` pairs a `Rect` with the
  target a click there means; `HitMap<T>` records one per clickable region each frame
  and resolves a click to the **topmost** matching box, so a modal's rows outrank the
  screen rows behind them. A modal groups its map with the panel it drew in a
  `ClickSurface<T>`, so recording rows and the outside-click rule are shared, not
  re-derived per modal. It is generic over the screen's own target, so policy — what a
  click *means* — stays in the screen.

Rules that follow from it:

- **A click runs the same dispatch as its key.** The Tree's per-mode handler was
  extracted to an `Action`-level `dispatch_action(action, state)`, and every modal
  grew a `handle_click` next to `handle_key`; a click maps to an `Action` (or a
  synthetic key) and runs the existing handler, so keyboard and mouse cannot diverge.
- **Soft policy vs. hard policy.** Two user-approved rules shape every click:
  a click first **moves the selection** to the clicked row, and a **repeat** click on
  an already-selected list row **activates** it. The first is the shared
  `hitbox::RowClick::resolve(selected, clicked)` resolver (`Moved`/`Activated`), so
  every picker reads the same rule; each site keeps its own activation branch. On the
  Tree a branch toggles on a single left click while a leaf is activated by the right
  button (which opens its Detail); a right click on a Detail field row copies the
  value (an empty value is a silent no-op).
- **An outside click is that modal's `Esc`.** A left click that misses a dismissable
  modal's panel runs the modal's own `Esc` handler — the same code path as the key —
  so a click closes a form, closes an export/share wizard, unwinds one nested popup
  (a nested confirmation steps back one level), discards Configuration edits, or
  declines a Confirmation. A click can still never answer `y`. A modal that has not
  been drawn yet records no panel, so a stray click cannot dismiss it. The File
  Browser follows the rule as well: its centered overlay is the panel, so a click
  beside it runs the browser's `Esc`, closing it or unwinding one of its popups.
- **Destructive prompts take no clicks.** The Tree's confirmation prompts
  (delete-field, share-without-password) record no hit-boxes, the File Browser's
  click handler returns early while a delete/overwrite confirm is pending (its list
  boxes are still recorded, but unreachable), and a Home `Confirmation` records
  nothing, so no click can answer `y`. Help/About/Alert dismiss on a left click
  anywhere; a wheel tick never does, because it must not be the key that closes them.
- **The wheel is a key.** `wheel_nav_key` maps `ScrollUp`/`ScrollDown` to `Up`/`Down`
  and the tick flows through the same dispatch, so every list scrolls identically in
  every terminal with no coordinate maths.
- **Renderers own the geometry.** Each renderer records its rows' rects as it draws
  them (the File Browser accounts for its `scroll`); the event loop asks the map, so
  the row a click lands on is the row that was drawn there.

## Trade-off accepted

While mouse capture is on, the terminal's **native text selection is off** (most
emulators still select while `Shift` is held). This was accepted deliberately: the
deterministic wheel and useful clicks were worth more than select-to-copy, which the
app already offers through its own clipboard gateway ([ADR 0025](0025-clipboard-gateway.md)).

## Not done

- Caret positioning by clicking inside a field (per-character hit-testing; odd for
  masked fields).
- Click-to-select/extend text, drag, double-click word selection.
- Clicks on the status bar or title bar. With a dismissable modal open, a click that
  lands there is simply outside the modal's panel and counts as its `Esc`.

## Consequences

- Two leaves join the module map; `terminal` is `pub(crate)` (it owns `Tui`) and
  `hitbox` is `pub(crate)` too — neither widens the crate's public contract.
- Hit-boxes touch every renderer, the largest mechanical cost of the feature;
  coordinate bugs are caught by `TestBackend` hit-testing tests.
