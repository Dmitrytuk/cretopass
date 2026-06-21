# simple-password-keeper

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
| `q` / `Ctrl+C` | Quit entire app |
| `H` (shift+h) | Return to settings menu |

Detail overlay: `j`/`k`=navigate rows, `Enter`=copy row data to clipboard, `i`=edit, `Esc`/`Space`=close. Shows extra fields (Login/Password/Custom) below description. Selected row highlighted with `▸`. Copy shows "Copied!" alert popup — any key dismisses it.

Add/Edit form: `Tab`/`Up`/`Down`=switch field, `Enter`=save, `Esc`=cancel (with confirmation if data changed), `Left`/`Right`/`Home`/`End`=cursor. Supports extra fields. `Esc` on changed data shows "Discard changes?" popup — `Enter`/`y` discards, `n`/`Esc` returns to editing.

Extra fields: each node can have Login, Password, and/or Custom fields. Focus "+ Add Field" row and press `Enter` to add one. Pick type from popup (`j/k` or `1`/`2`/`3` to navigate, `Enter` to choose). Custom fields prompt for a name first. Focus an extra field row and press `Del` to delete with confirmation (`y`).

Password field values display as `*****` in the detail overlay and when unfocused in edit forms. When actively editing (cursor in field), the raw value is shown.

## Architecture

- **Crate**: library + binary. `src/lib.rs` is the crate root, re-exporting public API. `src/main.rs` is a thin binary that launches the TUI and can be consumed as a dependency by other crates.
- **16 source files**:
  - `src/lib.rs`: crate root, re-exports, `build_demo_tree()`, terminal helpers, `run()`/`run_with_root()`
  - `src/main.rs`: calls `tree_tui::run()`
  - `src/cursor.rs`: shared UI helpers — `cursor_field_spans()`, `selected_span()`, `selected_style()`
  - `src/tree/mod.rs`: module decl
  - `src/tree/node.rs`: Node struct
  - `src/tree/widget.rs`: `TreeState`, `InputMode`, `TreeWidget` — also `current_node_ref()`, `find_node_ref()`
  - `src/tree/app_tree.rs`: event loop, key handlers, overlay rendering — uses merged `handle_form()` (replaces `handle_adding`/`handle_editing`), `handle_text_edit()` helper, `render_file_input()`, `current_node_ref()` for borrow-safe subtree access
  - `src/settings.rs`: module root, re-exports `ui`, `active_file`, and `utils`
  - `src/settings/ui.rs`: settings GUI — `SettingsState`, `run()`, `render()`
  - `src/settings/widget.rs`: `handle_modal_key` dispatch, 5 per-modal handlers (`handle_app_settings_key`, `handle_delete_confirm_key`, `handle_change_pw_confirm_key`, `handle_change_pw_input_key`, `handle_confirm_no_passphrase_key`), cursor helpers from `crate::cursor`
  - `src/settings/active_file.rs`: opened-file state — path, passphrase, Node
  - `src/settings/utils.rs`: orchestration — file validation, decrypt, save, recents, file dialog, settings persistence
  - `src/file_browser.rs`: module root
  - `src/file_browser/state.rs`: `FileBrowserState`, `FileBrowserMode`
  - `src/file_browser/render.rs`: all file browser rendering, `render_file_input()`
  - `src/file_browser/handler.rs`: file browser key handling — `handle_text_edit()` helper, input/rename handlers
  - `src/file_browser/fs.rs`: filesystem operations, `is_age_file()` header check
  - `src/file_handling/mod.rs`: JSON serialization, file I/O
  - `src/crypt/mod.rs`: age-based encrypt/decrypt
- **Node**: carries `id: Uuid`, `name`, `description`, `extra_fields: Vec<ExtraField>`, `children: Vec<Node>`
- **ExtraField/FieldType**: `{field_type: Login|Password|Custom(String), value: String}` — `display_value()` masks Password as `*****`
- **TreeState**: owns root Node, `expanded: HashSet<Uuid>`, `selected: usize`, clipboard, search state, `mode: InputMode`, `active_file_path`, `active_file_passphrase`, `dirty` flag
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

ratatui 0.28, crossterm 0.28, uuid 1.0 (features: v4, serde), serde 1 (feature: derive), serde_json 1, age 0.11.

## Settings Menu

- `cargo run` opens the settings menu first. Navigate with `j`/`k`, select with `Enter`, quit with `q`/`Esc`.
- "Open secret file" opens a **FileInput modal** to enter a path (or press Enter on empty field for OS file browser). Validates file (≤50MB, age-encrypted header), then opens a **PassphraseInput modal**. On success, launches the tree app with the decrypted data. Wrong passphrase shows a red error bar; modal stays open. `Esc` cancels.
- "Create new secret file" opens a **FileInput modal** to enter a path/filename. Validates writability via a temp-file probe, then opens the PassphraseInput modal. On success creates an encrypted file with a root node named `"Secrets"` and opens the tree app.
- **Recent files** loaded from `~/.config/pwd-keeper/recent_files.json`. Each row is selectable — Enter validates the file then opens the PassphraseInput modal.
- Active file path shown in the window title bar of both Settings and Tree (`" Settings — /path/to/file "` / `" Tree — /path/to/file "`).
- After successful open+decrypt, the file path is saved to recent files (deduplicated, most recent first).

## Auto-save

- Every mutation (add, edit, cut, paste) sets `TreeState.dirty = true`.
- On the next event-loop iteration, if a file is loaded (`active_file_path` is Some), `settings::utils::save_tree()` is called.
- `save_tree()`: (1) copies the current file to `{path}.bak`, (2) serialises `root` to JSON via `file_handling`, (3) encrypts with the stored passphrase via `crypt`, (4) writes to the original path.
- No explicit save keybinding needed — saves are automatic after every change.

## Quirks & edge cases

- Quit sets `state.quit` flag → event loop breaks → `main()` runs cleanup (LeaveAlternateScreen, disable_raw_mode, show_cursor). `q` in tree app quits everything (including settings). `H` (shift+h) in tree app returns to settings menu.
- Add form defaults empty name to `"Untitled"`.
- `remove_node()` only targets children (not root). Root cannot be cut.
- `insert_after()` finds parent, inserts at `pos+1`. `paste_child()` inserts as first child.
- `find_parent_mut()` uses `any()` on children ids (doesn't check root itself).
