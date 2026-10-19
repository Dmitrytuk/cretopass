# 0005 — tree module

The tree module is the core of the application: it owns the `Node` data model,
renders the tree UI, runs the event loop, and handles all tree-screen overlays
(search, detail, edit/add forms, export, help, confirm dialogs).

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
| `widget.rs` | `TreeState` (root, expanded set, selected index, visible list, mode, vault path, modified flag, save error, cut buffer, search state), `VisibleNode`, `InputMode` enum, `FormState` + undo/redo history, `TreeWidget::render()` DFS flat-list, all tree mutation operations |
| `app_tree.rs` | `run_app()` async event loop (`EventStream` + `tokio::select!`), key handlers per `InputMode`, autosave submission and exit flush, overlay rendering (search bar, detail, edit/add forms, help, export, confirm dialogs, save errors, status bar), clipboard integration |

## Data model

- **Node**: `{id: Uuid, name, extra_fields: Vec<ExtraField>, children: Vec<Node>}`
- **ExtraField**: `{field_type: Login | Password | Custom(String), value: String}`
- **TreeState**: owns root `Node`, `expanded: HashSet<Uuid>`, `selected: usize` (flat-list index), `cut_buffer: Option<Node>`, `mode: InputMode`, `active_vault_path`, `modified: bool`, `save_error: Option<String>`

### Description as an ordinary ExtraField

**Decision:** a Node has no dedicated description field. A description is a
regular ExtraField whose type is `Custom("Description")`, created with
`ExtraField::description(value)`. It is therefore subject to the same rules as
any other field: it can be added, reordered (`dd`/`p`), duplicated and deleted,
and a Node simply has no description until such a field is added.

**Rejected alternative:** keep `Node.description: String`. It forced a special
case through the form (`show_desc`, `FormRow::Desc`), the Detail overlay
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

## InputMode state machine

12 variants: `Normal`, `Search`, `PendingCut`, `PendingPaste`, `PendingAdd`,
`Detail{selected, alert}`,
`Form(FormState)`, `SelectFieldType{form, selected}`,
`CustomFieldName{form, input}`, `ConfirmDeleteField{form, target_index}`,
`ExportNodeSelect{...}`,
`ExportNodeFormat{...}`, `Alert{message}`, `Help`.

Each variant carries its transient state inline. Key handlers dispatch on the
current variant, transform state, and return the next mode.

All Add/Edit form state lives in a single `FormState` (`node_id`, `draft`,
`selected_row`, `cursor`, `focus`, `pending`, `field_cut_buffer`, `scratch`,
`history`, `history_cursor`), which the three popup variants carry as `form`.
This replaces the earlier duplication of the same fields in every variant.

## Prefix modes

`PendingCut`, `PendingPaste`, and `PendingAdd` are one-key prefix modes:

- `d` enters `PendingCut`; a second `d` cuts, any other key cancels.
- `p` enters `PendingPaste` only when the cut buffer is non-empty; `p`/`P`/`l`(→)
  choose paste after/before/child, any other key cancels.
- `a` enters `PendingAdd`; `a`/`A`/`l`(→) choose add after/before/child,
  any other key cancels. The choice is stored in `TreeState::add_position`
  (`AddPosition`) and consumed when the form is confirmed.

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
- Closing the form: an Edit session (including a created Add session) returns to
  the node's `Detail` overlay; an empty Add session returns to `Normal`. There
  is no discard prompt — every change is already committed.
- Password values render as `*****` at every level except an actively edited
  (`Input`) row.
- Delete on extra field: `Delete` key → `ConfirmDeleteField` popup → `y`
  confirms, any other key cancels. A confirmed deletion commits immediately.
- Field reordering: `dd` moves the focused extra field into
  `FormState::field_cut_buffer`; `p` / `P` paste it after / before the focused
  field. The prefix state is `FormState::pending` (`FormPending`), so no new
  `InputMode` is introduced; any other key cancels the prefix. `Name` and
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
- System clipboard only used in detail overlay (Enter on a row). Targets: `pbcopy` (macOS), `wl-copy`/`xclip`/`xsel` (Linux).
- `remove_node()` only targets children (not root). Root cannot be cut.

## Autosave flow

1. A mutation (add/edit/cut/paste/undo/redo) commits directly into the root and
   sets `TreeState.modified = true`.
2. Before the next draw, the event loop clones the root and submits it to the
   `file_handling::Autosave` worker, receiving a generation number. `modified`
   is then cleared: it means "not yet submitted", not "not yet written".
3. The worker serializes saves on one thread. A submission made while a save is
   running replaces any still-waiting snapshot; the newest root therefore wins
   and a burst of edits costs at most one extra save.
4. The status bar shows "Saving..." from submission until the event for the
   latest generation arrives. On failure it shows "Save failed"; the full text
   is stored in `TreeState.save_error` and drawn as an overlay. Any key
   dismisses the overlay.
5. On quit (`q`) or return-to-home (`H`), the loop retries the final snapshot if
   the last attempt failed or `modified` is set, then waits for the latest
   generation. A failed final save cancels the transition and leaves the user in
   the Tree with the error overlay; the caller only leaves the Tree after the
   vault is up to date.

The master passphrase is owned by the worker closure, not by `TreeState`.
`run_app()` accepts it as an `Option<Zeroizing<Vec<u8>>>`; the library demo runs
without one and therefore without a worker. See
[ADR 0010 — Asynchronous tree autosave](../../../../docs/adr/0010-async-tree-autosave.md)
for the worker contract and the latest-wins rationale.

## Edge cases

- `find_parent_mut()` uses `any()` on children ids — does not check root itself.
- `insert_after()` finds parent, inserts at `pos+1`.
- `paste_child()` inserts as first child.
- Export formats: JSON (serde), XML (hand-rolled recursive with escaping), CSV (flattened with `path > child` hierarchy).

## Dependencies

- `uuid` 1.0 — node identity
- `serde` 1 + `serde_json` 1 — serialization
- `zeroize` — passphrase ownership at the vault/tree boundary
- `tokio` 1 (`rt`, `sync`, `macros`) + `futures-util` 0.3 — async input stream and save-event channel
- `file_handling` — autosave worker, export
- `cursor` — text editing helpers
- `theming` — Action dispatch, strings, styles