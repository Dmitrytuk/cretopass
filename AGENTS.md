# tree-tui

Rust ratatui-based TUI app with a custom tree data structure (not file-system), vim-style navigation, search/filter, and edit/add forms. In-memory only, no persistence.

## Build & Run

```sh
cargo build                          # debug build
cargo run                            # launch TUI
cargo doc --open                     # generate and open API docs from doc comments
```

## Keybindings

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate up/down |
| `h`/`l` | Collapse/expand (leaf: `l` opens detail) |
| `Enter` | Toggle expand (parent) or detail (leaf) |
| `Ctrl+Enter` / `i` | Detail overlay for any node |
| `Enter` (detail) | Copy selected row to clipboard |
| `d d` | Cut selected node |
| `p` / `P` | Paste as sibling / as child |
| `o` | Add child |
| `/` | Search (type, `Enter` confirm, `Esc` cancel) |
| `Esc` (normal) | Clear search highlights |
| `q` / `Ctrl+C` | Quit |

Detail overlay: `j`/`k`=navigate rows, `Enter`=copy row data to clipboard, `i`=edit, `Esc`/`Space`=close. Shows extra fields (Login/Password/Custom) below description. Selected row highlighted with `▸`. Copy shows "Copied!" alert popup — any key dismisses it.

Add/Edit form: `Tab`/`Up`/`Down`=switch field, `Enter`=save, `Esc`=cancel (with confirmation if data changed), `Left`/`Right`/`Home`/`End`=cursor. Supports extra fields. `Esc` on changed data shows "Discard changes?" popup — `Enter`/`y` discards, `n`/`Esc` returns to editing.

Extra fields: each node can have Login, Password, and/or Custom fields. Focus "+ Add Field" row and press `Enter` to add one. Pick type from popup (`j/k` or `1`/`2`/`3` to navigate, `Enter` to choose). Custom fields prompt for a name first. Focus an extra field row and press `Del` to delete with confirmation (`y`).

Password field values display as `*****` in the detail overlay and when unfocused in edit forms. When actively editing (cursor in field), the raw value is shown.

## Architecture

- **Crate**: library + binary. `src/lib.rs` is the crate root, re-exporting public API. `src/main.rs` is a thin binary that launches the TUI and can be consumed as a dependency by other crates.
- **7 source files**: `src/lib.rs` (crate root, re-exports, `build_demo_tree()`, terminal helpers, `run()`/`run_with_root()`), `src/main.rs` (just calls `tree_tui::run()`), `src/tree/mod.rs` (module decl), `src/tree/node.rs` (Node struct), `src/tree/widget.rs` (TreeState, InputMode, TreeWidget), `src/tree/app_tree.rs` (event loop, key handlers, all overlay rendering), `src/settings/mod.rs` (settings menu, `run()`, `render()`)
- **Node**: carries `id: Uuid`, `name`, `description`, `extra_fields: Vec<ExtraField>`, `children: Vec<Node>`
- **ExtraField/FieldType**: `{field_type: Login|Password|Custom(String), value: String}` — `display_value()` masks Password as `*****`
- **TreeState**: owns root Node, `expanded: HashSet<Uuid>`, `selected: usize`, clipboard, search state, `mode: InputMode`
- **InputMode**: Normal, Search, AwaitingDd, Detail, Adding{name,desc,extra_fields,field,cursor,delete_pending}, Editing{node_id,...}, SelectFieldType, CustomFieldName, ConfirmDeleteField
- **TreeWidget::render**: DFS flat list with indentation, ▶/▼ icons, ▸ selection arrow, search highlight (DarkGray bg)

## Editing quirks

- Cursor is a **byte offset** (not grapheme). Editing uses `String::insert(byte_pos, c)` and `String::remove(byte_pos)`. Multi-byte chars handled via `chars().next().len_utf8()` in `cursor_field_spans()`.
- `cursor_field_spans()` splits text at cursor, renders character under cursor as inverted rectangle (White bg, Black fg).
- Extra fields use `field` index scheme: `0`=name, `1`=desc, `2..2+n-1`=extra fields, `2+n`="+ Add Field" button.
- `update_node()` and `add_child_to_selected()` now accept `extra_fields: Vec<ExtraField>`.
- Adding mode also supports cursor and extra fields (same as editing mode).
- Delete on extra field: `Delete` key opens confirmation popup; `y` confirms, any other key cancels.

## Search

- `set_search()` auto-expands ancestors of matches via `expand_for_search()` before building visible list.
- j/k constrained to matched nodes only while search mode is active.
- Enter keeps highlights but frees navigation. Esc clears everything.

## Clipboard (cut/paste)

- `clipboard: Option<Node>` is copy-style (can paste multiple times).
- System clipboard only used in detail overlay (`c` key). Targets: `pbcopy` (macOS), `wl-copy`/`xclip`/`xsel` (Linux).

## Dependencies

ratatui 0.28, crossterm 0.28, uuid 1.0 (feature: v4). No test framework, no lint config, no CI.

## Settings Menu

- `cargo run` opens the settings menu first. Navigate with `j`/`k`, select with `Enter`, quit with `q`/`Esc`.
- "Open secret file" launches the tree app with the built-in demo tree (Knowledge Base). When the tree app quits, control returns to the settings menu.
- "Create new secret file" is a placeholder (no action yet).
- Recent files list shows dummy data (`vault_2024.kdbx`, `work_passwords.kdbx`, `personal.kdbx`).

## Quirks & edge cases

- Quit sets `state.quit` flag → event loop breaks → `main()` runs cleanup (LeaveAlternateScreen, disable_raw_mode, show_cursor).
- Add form defaults empty name to `"Untitled"`.
- `remove_node()` only targets children (not root). Root cannot be cut.
- `insert_after()` finds parent, inserts at `pos+1`. `paste_child()` inserts as first child.
- `find_parent_mut()` uses `any()` on children ids (doesn't check root itself).
