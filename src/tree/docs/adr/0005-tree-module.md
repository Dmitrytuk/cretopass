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
| `node.rs` | `Node` struct (id, name, description, children, extra_fields), `ExtraField`, `FieldType` enum, recursive `find`/`find_mut`/`contains`, wire-label helpers for import/export |
| `widget.rs` | `TreeState` (root, expanded set, selected index, visible list, mode, vault path/passphrase, modified flag, cut buffer, search state), `VisibleNode`, `InputMode` enum (16 variants), `TreeWidget::render()` DFS flat-list, all tree mutation operations |
| `app_tree.rs` | `run_app()` event loop, key handlers per `InputMode`, overlay rendering (search bar, detail, edit/add forms, help, export, confirm dialogs, alerts, status bar), clipboard integration |

## Data model

- **Node**: `{id: Uuid, name, description, extra_fields: Vec<ExtraField>, children: Vec<Node>}`
- **ExtraField**: `{field_type: Login | Password | Custom(String), value: String}`
- **TreeState**: owns root `Node`, `expanded: HashSet<Uuid>`, `selected: usize` (flat-list index), `cut_buffer: Option<Node>`, `mode: InputMode`, `active_vault_path`, `active_vault_master_password` (Zeroizing), `modified: bool`

## InputMode state machine

13 variants: `Normal`, `Search`, `PendingCut`, `PendingPaste`, `PendingAdd`,
`Detail{selected, alert}`,
`Form(FormState)`, `SelectFieldType{form, selected}`,
`CustomFieldName{form, input}`, `ConfirmDeleteField{form, target_index}`,
`ConfirmDiscardChanges(FormState)`, `ExportNodeSelect{...}`,
`ExportNodeFormat{...}`, `Alert{message}`, `Help`.

Each variant carries its transient state inline. Key handlers dispatch on the
current variant, transform state, and return the next mode.

All Add/Edit form state lives in a single `FormState` (`node_id`, `name`,
`desc`, `show_desc`, `extra_fields`, `selected_row`, `cursor`, `focus`,
`pending`, `field_cut_buffer`), which the four popup variants carry as `form`.
This replaces the earlier duplication of the same six fields in every variant.

## Prefix modes

`PendingCut`, `PendingPaste`, and `PendingAdd` are one-key prefix modes:

- `d` enters `PendingCut`; a second `d` cuts, any other key cancels.
- `p` enters `PendingPaste`; `p`/`P`/`l`(→) choose paste after/before/child,
  any other key cancels.
- `a` enters `PendingAdd`; `a`/`A`/`l`(→) choose add after/before/child,
  any other key cancels. The choice is stored in `TreeState::add_position`
  (`AddPosition`) and consumed when the form is confirmed.

`h`/`←` collapses an expanded parent; otherwise it moves the cursor to the
parent row (`collapse_or_select_parent`).

## Editing quirks

- Add and Edit share one form model: `FormState`. Add is `node_id == None`;
  Edit is `node_id == Some(id)`.
- Row order is derived, not hard-coded: `FormState::rows()` returns
  `Name`, optional `Desc`, one `Extra(i)` per field, then `AddButton`.
  `selected_row` indexes that list, and `clamp_selection()` keeps it valid
  after the list changes.
- A new node starts with **only the Name row**. The Description row is added
  through `+ Add Field` → `Description` (a picker item shown only while the Add
  form has no description yet). `Node.description` is unchanged on disk.
- The form is two-level, tracked by `FormState::focus` (`FormFocus::Nav` /
  `FormFocus::Input`). It opens in `Nav`; `i` enters `Input`, `Esc` returns to
  `Nav`. Popups (field-type picker, custom name, delete/discard confirms) are
  only reachable from `Nav` and return to `Nav`.
- `Nav`: `j`/`k`/arrows/`Tab`/`Shift+Tab` move between rows, `Enter` saves (or
  opens the field-type picker on the `+ Add Field` row), `Delete` on an extra
  field row opens the confirm-delete popup.
- `Input`: printable keys edit the focused row's text; the cursor is shown only
  at this level. In `Nav` the focused row uses the same highlight as Detail
  (`▸ ` + `FormRowFocused`).
- Empty name defaults to `"Untitled"` on save.
- Password values render as `*****` at every level except an actively edited
  (`Input`) row.
- Delete on extra field: `Delete` key → `ConfirmDeleteField` popup → `y`
  confirms, any other key cancels.
- Field reordering: `dd` moves the focused extra field into
  `FormState::field_cut_buffer`; `p` / `P` paste it after / before the focused
  field. The prefix state is `FormState::pending` (`FormPending`), so no new
  `InputMode` is introduced; any other key cancels the prefix. `Name`, `Desc`
  and `AddButton` are never movable, and paste is only allowed with an
  `Extra(i)` row focused. The buffer lives and dies with the form, so `Esc`
  discards all unsaved reordering.

## Search

- `set_search()` auto-expands ancestors of matches via `expand_for_search()` before building the visible list.
- While in Search mode, typing edits the query; `Enter` returns to Normal keeping highlights, `Esc` clears everything.
- While a search is active (Normal mode), j/k cycle through matched nodes only.
- `Delete` is not bound in Search — use `Backspace` to erase query chars.

## Cut buffer

- `cut_buffer: Option<Node>` is copy-style (can paste multiple times).
- `p` enters `PendingPaste`; `p` pastes after, `P` pastes before, `l`/`→`
  pastes as last child.
- System clipboard only used in detail overlay (Enter on a row). Targets: `pbcopy` (macOS), `wl-copy`/`xclip`/`xsel` (Linux).
- `remove_node()` only targets children (not root). Root cannot be cut.

## Autosave flow

1. Mutation (add/edit/cut/paste) sets `TreeState.modified = true`.
2. Next event-loop tick: if `active_vault_path.is_some()`, call `file_handling::save_tree()`.
3. Status bar shows "Saving..." during the save.
4. Quit (`q`) or return-to-home (`H`) does explicit save before transitioning.

## Edge cases

- `find_parent_mut()` uses `any()` on children ids — does not check root itself.
- `insert_after()` finds parent, inserts at `pos+1`.
- `paste_child()` inserts as first child.
- Export formats: JSON (serde), XML (hand-rolled recursive with escaping), CSV (flattened with `path > child` hierarchy).

## Dependencies

- `uuid` 1.0 — node identity
- `serde` 1 + `serde_json` 1 — serialization
- `zeroize` — master passphrase in TreeState
- `file_handling` — autosave, export
- `cursor` — text editing helpers
- `theming` — Action dispatch, strings, styles