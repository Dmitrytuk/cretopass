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

14 variants: `Normal`, `Search`, `AwaitingDd`, `Detail{selected, alert}`,
`Editing{node_id, name, desc, extra_fields, field, cursor}`,
`Adding{name, desc, extra_fields, field, cursor}`,
`SelectFieldType{...}`, `CustomFieldName{...}`, `ConfirmDeleteField{...}`,
`ConfirmDiscardChanges{...}`, `ExportNodeSelect{...}`, `ExportNodeFormat{...}`,
`Alert{message}`, `Help`.

Each variant carries its transient state inline. Key handlers dispatch on the
current variant, transform state, and return the next mode.

## Editing quirks

- Adding and Editing modes share the same form structure: `name`, `desc`, `extra_fields`, `field` index, `cursor` byte-offset.
- Extra fields use a `field` index scheme: `0` = name, `1` = desc, `2..2+n-1` = extra fields, `2+n` = "+ Add Field" button.
- Empty name defaults to `"Untitled"` on save.
- Password field values display as `*****` when unfocused; raw value when cursor is in the field.
- Delete on extra field: `Delete` key → `ConfirmDeleteField` popup → `y` confirms, any other key cancels.

## Search

- `set_search()` auto-expands ancestors of matches via `expand_for_search()` before building the visible list.
- j/k constrained to matched nodes only while search mode is active.
- Enter keeps highlights but frees navigation. Esc clears everything.

## Cut buffer

- `cut_buffer: Option<Node>` is copy-style (can paste multiple times).
- `p` pastes as sibling, `P` pastes as child.
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