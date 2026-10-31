# 0005 — tree module

The tree module is the core of the application: it owns the `Node` data model,
renders the tree UI, runs the event loop, and hosts the Tree's input-capturing
windows. Those are `Modal`s — Detail (with the Add/Edit `Form` Popup nested in
it), Help, Alert, Export, and Share — over a small set of navigation *modes*
(Normal, Search, the cut/paste/add/export prefixes).

**Decision:** represent the tree as a recursive `Node` struct and flatten it to
a `Vec<VisibleNode>` via DFS for rendering. Navigation happens on the flat index
list, not on the tree itself. This avoids recursive rendering logic in ratatui
and simplifies search/match highlighting.

**Rejected alternative:** nested ratatui layouts or custom recursive widgets.
Would complicate scrolling, search, and keyboard navigation for no benefit.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `node.rs` | `Node` struct (id, name, children, extra_fields), `ExtraField`, `FieldType` enum, legacy-`description` migration, recursive `find`/`find_mut`/`contains`, wire-label helpers for import/export |
| `widget.rs` | module facade: declares the sub-modules and re-exports the public API (`TreeState`, `InputMode`, `SaveState`, `TreeWidget`), the internal API (`Modal`, `TreeAction`, `Detail`, `FormState`, …, `execute`) and the test-only helpers. Holds no logic |
| `widget/state.rs` | the tree model and its operations: `TreeState` (root, expanded set, selected index, visible list, navigation mode, vault path, modified flag, cut buffer, search state, `SaveState`, `SaveExit`, the `Modal` slot, the File Browser session memory), `VisibleNode`, the five-variant `InputMode` enum, `AddPosition`, and all tree mutation, navigation and search operations |
| `widget/form.rs` | the Add/Edit form: `FormState` + undo/redo history (`Snapshot`, `Scratch`), the `FormRow`/`FormFocus`/`FormPending` enums, and `GeneratorModal` |
| `widget/save.rs` | the autosave status `SaveState` and the parked exit `SaveExit` |
| `widget/render.rs` | `TreeWidget::render()`: the DFS flat-list view and its title bar |
| `widget/modal.rs` | the `Modal` slot, the `TreeAction` intent enum, and the `handle_key(key, &mut TreeState) -> Option<TreeAction>` contract; declares the per-modal sub-modules |
| `widget/modal/*.rs` | one file per modal (or popup): `help`, `alert`, `detail` (the Form and its picker/name/confirm/generator popups), `export`, `export_destination` (the File Browser naming an export's destination); `chrome.rs` holds the shared overlay geometry |
| `widget/orchestrator.rs` | the one place a `TreeAction` becomes real work: system clipboard, export file write, screen transitions |
| `app_tree.rs` | `run_app()` async event loop (`EventStream` + `tokio::select!`), navigation-mode key handlers, routing to the active modal, autosave submission, bounded exit flush with the force-quit escape hatch, search overlay, save-error and status-bar rendering |

## Data model

- **Node**: `{id: Uuid, name, extra_fields: Vec<ExtraField>, children: Vec<Node>}`
- **ExtraField**: `{field_type: Login | Password | Custom(String), value: String}`
- **TreeState**: owns root `Node`, `expanded: HashSet<Uuid>`, `selected: usize` (flat-list index), `cut_buffer: Option<Node>`, `mode: InputMode`, `active_vault_path`, `modified: bool`, `save_state: SaveState`, `save_exit: SaveExit`, `generator: PasswordParams` (per-run generator memory seeded by the vault)

### Description as an ordinary ExtraField

**Decision:** a Node has no dedicated description field. A description is a
regular ExtraField whose type is `Custom("Description")`, created with
`ExtraField::description(value)`. It is therefore subject to the same rules as
any other field: it can be added, reordered (`dd`/`p`), duplicated and deleted,
and a Node simply has no description until such a field is added.

**Rejected alternative:** keep `Node.description: String`. It forced a special
case through the form (`show_desc`, `FormRow::Desc`), the Detail modal
(hard-coded row 0) and every import/export format (a dedicated
`description` attribute/column), and made "no description" indistinguishable
from "empty description".

**Consequence — format change:** the serialized Node no longer has a
`description` key. Backward compatibility is handled by a custom `Deserialize`
in `node.rs` (`NodeRepr` + `extra_fields.insert(0, ExtraField::description(..))`):
a non-empty legacy `description` is migrated into a leading Description field;
an empty or absent one yields no field. The same migration is applied by the
XML/CSV importers (`migrate_legacy_description`), so old interchange files keep
their descriptions. Once a migrated vault is saved, the legacy key is gone.

## Navigation modes

`InputMode` is a six-variant enum of navigation modes, each carrying no
payload: `Normal`, `Search`, `PendingCut`, `PendingPaste`, `PendingAdd`,
`PendingExport`. They change how a key is interpreted in the tree itself; none
of them is a window.

Each mode has one handler in `app_tree.rs`; the handlers transform state and set
the next mode (or open a modal).

## Modals

Every input-capturing window is a `Modal` in the `TreeState::modal` slot — at
most one at a time. `Modal::handle_key` reads a key and returns a `TreeAction`;
`orchestrator::execute` performs it. A modal may read and mutate the tree (the
Detail modal's Form Popup live-edits a Node), but anything outside the process —
the system clipboard, the export file write, screen transitions — is a
`TreeAction`. This is the same intent contract the Home screen uses (see
[0021](../../../../docs/adr/0021-home-intent-contract.md)); `Modal` and `Popup`
are the vocabulary of [GLOSSARY.md](../../GLOSSARY.md). Because the modals are
pure over their own state, their `handle_key` is testable without a terminal.

- `Modal::Detail` browses a Node's extra fields (copying a value to the
  clipboard is its only I/O). Its Form Popup carries all Add/Edit state in a
  single `FormState` (`node_id`, `draft`, `selected_row`, `cursor`, `focus`,
  `pending`, `field_cut_buffer`, `scratch`, `history`, `history_cursor`,
  `history_coalesce`), with the field-type picker, custom-name prompt, delete
  confirmation, and the generator (parameter modal + symbol checklist) nested
  inside it. `Ctrl+G` at the Form's Input level dispatches on the focused row's
  field type: a `Password` field opens the generator popup (see
  [0012](../../../../docs/adr/0012-password-generator.md)), while a `Login`
  field generates a word immediately (see
  [0013](../../../../docs/adr/0013-login-word-generator.md)).
- `Modal::Help` lists the key bindings; any key closes it.
- `Modal::Alert` shows a message; any key closes it.
- `Modal::Export` is the two-step wizard (scope, then format); the destination is
  named afterwards in the File Browser ([`Modal::ExportDestination`]), whose
  confirmed path reports `TreeAction::Export`, which the orchestrator writes to
  disk. See [0022](../../../../docs/adr/0022-user-chosen-export-destination.md).
- The share flow is three modals: `Modal::Share` (a one-step picker of what to
  share — `This node` / `This node with children`, the same choice as export),
  then `Modal::ShareDestination` (the File Browser naming the bundle folder),
  then `Modal::SharePassphrase` (the recipient's masked password pair, with an
  inline `y`-confirm for a passwordless bundle). The confirmed bundle reports
  `TreeAction::Share`, which the orchestrator writes through
  `file_handling::share`; a node shared without its children drops them, exactly
  like an export. See [0023](../../../../docs/adr/0023-sharing-a-vault-subtree.md).

## Prefix modes

`PendingCut`, `PendingPaste`, `PendingAdd`, and `PendingExport` are one-key
prefix modes:

- `d` enters `PendingCut`; a second `d` cuts, any other key cancels. The Root
  has no sibling slot, so `d` is inert while the Root is selected.
- `p` enters `PendingPaste` only when the cut buffer is non-empty; `p`/`P`/`l`(→)
  choose paste after/before/child, any other key cancels. On the Root only
  `l`(→) applies; `p`/`P` are no-ops.
- `a` enters `PendingAdd`; `a`/`A`/`l`(→) choose add after/before/child,
  any other key cancels. On the Root only `l`(→) is offered: `a`/`A` cancel
  instead of opening a form whose first commit could never insert. The choice
  is stored in `TreeState::add_position` (`AddPosition`) and consumed when the
  form is confirmed.
- `Ctrl+E` enters `PendingExport`; `e` keeps the export wizard, `s` opens the
  share wizard, any other key cancels.

`h`/`←` collapses an expanded parent; otherwise it moves the cursor to the
parent row (`collapse_or_select_parent`).

## Editing quirks

- Add and Edit share one form model: `FormState`. Add is `node_id == None`;
  Edit is `node_id == Some(id)`.
- The form is a **live editor** reading and writing the node directly, the
  single source of truth. Edit holds no copy of the node's data; Add keeps a
  pre-creation `draft` node until the first commit inserts it into the tree.
- Row order is derived, not hard-coded: `FormState::rows(root)` returns
  `Name`, one `Extra(i)` per field, then `AddButton`.
  `selected_row` indexes that list, and `clamp_selection(root)` keeps it valid
  after the list changes.
- A new node starts with **only the Name row**; a Node's description is added
  the same way as any other field, through `+ Add Field` → `Description` (a
  picker item that appends `Custom("Description")` and focuses it for input).
  Editing an existing node mirrors this: existing fields (including a migrated
  Description) show up as ordinary `Extra(i)` rows.
- The form is two-level, tracked by `FormState::focus` (`FormFocus::Nav` /
  `FormFocus::Input`). It opens in `Nav`; `i` and `Enter` enter `Input`.
- **Commit on Input exit:** while in `Input`, typing edits `FormState::scratch`
  and leaves the node untouched. `Enter` and `Esc` both leave `Input` and, when
  the scratch text differs, write it into the node, autosave, and push a history
  entry. Leaving with no change is a no-op.
- **Add becomes Edit:** the first committed change in the Add form (a changed
  field *or* a structural change) inserts the draft at `add_position`, expands
  the parent, selects the new node, switches `node_id` to that node, and seeds
  the undo history. From then on the session is an Edit session. `Esc` before
  any commit cancels the Add. An empty name defaults to `"Untitled"` on
  creation.
- `Nav`: `j`/`k`/arrows/`Tab`/`Shift+Tab` move between rows, `i`/`Enter` enter
  `Input` (or open the field-type picker on the `+ Add Field` row), `u`/`r`
  undo/redo, `Esc` closes the form. `Delete` on an extra field row opens the
  confirm-delete popup.
- Closing the form: an Edit session (including a created Add session) drops back
  to browsing the node in the `Detail` modal; an empty Add session closes the
  modal back to `Normal`. There is no discard prompt — every change is already
  committed.
- Password values render as `*****` at every level except an actively edited
  (`Input`) row.
- Delete on extra field: `Delete` key → `ConfirmDeleteField` popup → `y`
  confirms, any other key cancels. A confirmed deletion commits immediately.
- Field reordering: `dd` moves the focused extra field into
  `FormState::field_cut_buffer`; `p` / `P` paste it after / before the focused
  field. The prefix state is `FormState::pending` (`FormPending`), so the open
  screen never changes; any other key cancels the prefix. `Name` and
  `AddButton` are never movable, and paste is only allowed with an
  `Extra(i)` row focused. Each cut/paste is a live commit.

## Undo / redo

- The Edit form keeps an undo history: `FormState::history` is a `Vec<Snapshot>`
  (`Snapshot` = a node's `name` + `extra_fields`) with a `history_cursor`.
  `history[history_cursor]` is the current state.
- The baseline is seeded when the session opens (or, for a created Add session,
  at creation). Each committed change pushes a new snapshot after the cursor,
  discarding any redo tail.
- `u` moves the cursor back one and applies the previous snapshot; `r` moves it
  forward and re-applies the next. Both write straight into the node
  (autosave), clamp the selection, and **clear `field_cut_buffer`** (it may
  reference fields the restored state no longer has). Both are no-ops at the
  ends of the history.
- `u`/`r` are bound only at the `Nav` level and exist only for Edit sessions
  (Add has no history until its first commit creates the node).
- The history is discarded when the form is closed.

## Search

- `set_search()` auto-expands ancestors of matches via `expand_for_search()` before building the visible list.
- While in Search mode, typing edits the query; `Enter` returns to Normal keeping highlights, `Esc` clears everything.
- While a search is active (Normal mode), j/k cycle through matched nodes only.
- `Delete` is not bound in Search — use `Backspace` to erase query chars.

## Cut buffer

- `cut_buffer: Option<Node>` is move-style: a cut node is consumed by the next
  successful paste, so it can be inserted only once. A failed paste (e.g. `P`
  on the root, which has no sibling slot) puts the node back in the buffer.
- `p` enters `PendingPaste` only when the buffer is non-empty; `p` pastes after,
  `P` pastes before, `l`/`→` pastes as last child.
- System clipboard only used in the Detail modal (copy a row's value). Targets: `pbcopy` (macOS), `wl-copy`/`xclip`/`xsel` (Linux).
- `remove_node()` only targets children (not root). The Root cannot be cut, so
  `d` is inert while the Root is selected.

## Autosave flow

1. A mutation (add/edit/cut/paste/undo/redo) commits directly into the root and
   sets `TreeState.modified = true`.
2. Before the next draw, the event loop clones the root and submits it to the
   `file_handling::Autosave` worker, receiving a generation number. `modified`
   is then cleared: it means "not yet submitted", not "not yet written".
3. The worker serializes saves on one thread. A submission made while a save is
   running replaces any still-waiting snapshot; the newest root therefore wins
   and a burst of edits costs at most one extra save.
4. The title bar shows `Saving` (amber) with an animated glyph from submission
   until the event for the latest generation arrives. On failure it shows
   `Save failed` (red), driven by `TreeState.save_state: SaveState`; an
   unacknowledged failure also draws an overlay with the full error. Any key
   dismisses the overlay.
5. On quit (`q`) or return-to-home (`H`) the loop decides via the pure
   `exit_plan` helper, retrying the final snapshot if the last attempt failed or
   `modified` is set, then waiting for the latest generation. The wait is bounded
   by `SAVE_TIMEOUT`; on failure or timeout the exit is parked in
   `SaveExit::Blocked` and the user is offered retry (`r`/`Enter`) or an explicit
   confirmed quit-without-saving (`q` → `[y/N]`), so a persistently failing save
   can never trap them in the Tree. A force-quit returns `ExitStatus::Unsaved`,
   which makes Home drop the active vault rather than overwrite the on-disk copy.
   See [ADR 0010 — Asynchronous tree autosave](../../../../docs/adr/0010-async-tree-autosave.md#event-loop-contract)
   for the full overlay state machine.

The master passphrase and the vault path travel together in a
`tree::app_tree::SaveTarget`; `run_app()` accepts it as an `Option<SaveTarget>`.
`None` means a read-only tree (the library demo) and therefore no worker. See
[ADR 0010 — Asynchronous tree autosave](../../../../docs/adr/0010-async-tree-autosave.md)
for the worker contract and the latest-wins rationale.

## Edge cases

- `find_parent_mut()` uses `any()` on children ids — does not check root itself.
- `insert_after()` finds parent, inserts at `pos+1`.
- `paste_child()` inserts as last child.
- Export formats: JSON (serde), XML (hand-rolled recursive with escaping), CSV (flattened with `path > child` hierarchy).

## Dependencies

- `uuid` 1.0 — node identity
- `serde` 1 + `serde_json` 1 — serialization
- `zeroize` — passphrase ownership at the vault/tree boundary
- `tokio` 1 (`rt`, `sync`, `macros`, `time`) + `futures-util` 0.3 — async input stream, save-event channel, bounded exit flush
- `file_handling` — autosave worker, export
- `cursor` — text editing helpers
- `theming` — Action dispatch, strings, styles