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
| `d d` | Cut selected node |
| `p` / `P` | Paste as sibling / as child |
| `o` | Add child |
| `/` | Search (type, `Enter` confirm, `Esc` cancel) |
| `Esc` (normal) | Clear search highlights |
| `q` / `Ctrl+C` | Quit |

Detail overlay: `i`=edit, `c`=copy to clipboard, `Esc`/`Enter`/`Space`=close

Add/Edit form: `Tab`/`Up`/`Down`=switch field, `Enter`=save, `Esc`=cancel, `Left`/`Right`/`Home`/`End`=cursor (edit only)

## Architecture

- **4 source files**: `src/main.rs` (event loop + overlays), `src/tree/mod.rs` (module decl), `src/tree/node.rs` (Node struct), `src/tree/widget.rs` (TreeState, InputMode, TreeWidget)
- **Node**: carries `id: Uuid`, `name`, `description`, `children: Vec<Node>`
- **TreeState**: owns root Node, `expanded: HashSet<Uuid>`, `selected: usize`, clipboard, search state, `mode: InputMode`
- **InputMode**: Normal, Search, AwaitingDd, Detail, Adding{name,desc,field}, Editing{node_id,name,desc,field,cursor}
- **TreeWidget::render**: DFS flat list with indentation, ▶/▼ icons, ▸ selection arrow, search highlight (DarkGray bg)

## Editing quirks

- Cursor is a **byte offset** (not grapheme). Editing uses `String::insert(byte_pos, c)` and `String::remove(byte_pos)`. Multi-byte chars handled via `chars().next().len_utf8()` in `cursor_field_spans()`.
- `cursor_field_spans()` splits text at cursor, renders character under cursor as inverted rectangle (White bg, Black fg).

## Search

- `set_search()` auto-expands ancestors of matches via `expand_for_search()` before building visible list.
- j/k constrained to matched nodes only while search mode is active.
- Enter keeps highlights but frees navigation. Esc clears everything.

## Clipboard (cut/paste)

- `clipboard: Option<Node>` is copy-style (can paste multiple times).
- System clipboard only used in detail overlay (`c` key). Targets: `pbcopy` (macOS), `wl-copy`/`xclip`/`xsel` (Linux).

## Dependencies

ratatui 0.28, crossterm 0.28, uuid 1.0 (feature: v4). No test framework, no lint config, no CI.

## Quirks & edge cases

- Quit sets `state.quit` flag → event loop breaks → `main()` runs cleanup (LeaveAlternateScreen, disable_raw_mode, show_cursor).
- Add form defaults empty name to `"Untitled"`.
- `remove_node()` only targets children (not root). Root cannot be cut.
- `insert_after()` finds parent, inserts at `pos+1`. `paste_child()` inserts as first child.
- `find_parent_mut()` uses `any()` on children ids (doesn't check root itself).
