# 0007 — file_browser module

An overlay directory browser used by the Home screen for file selection (open,
create, import). Renders a scrollable file list with path bar, search filter,
hidden-entry toggle, mounted-disk locations, and contextual actions (rename,
delete, new folder).

**Decision:** keep the file browser as a separate module with its own state,
handler, and renderer, rather than embedding it in `vault::widget`. The file
browser has a self-contained state machine (`FileBrowserState` + `FileBrowserMode`)
and returns a `FileBrowserAction` result, making it reusable across multiple
vault modals.

**Rejected alternative:** inline file listing in vault. Would couple the
file listing UX to vault-specific modal logic, preventing reuse.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `state.rs` | `FileBrowserState`, `FileBrowserMode` (OpenFile/CreateFile/ImportFile), `FileBrowserSession` (last directory + hidden setting), `Location` (Home or mounted disk row), `FocusTarget` (List/Input/Rename/Locations), `NewFilePrefix` (None/Pending), `FileFilter` (All/PwdKeeperOnly), `FileBrowserAction` (Selected/Cancelled/Renamed) |
| `handler.rs` | `handle_key()` dispatches to list/input/rename/locations handlers. Supports j/k navigation, Enter, search, filter toggle, hidden-entry toggle (`Ctrl+h`), directory traversal, rename, delete, the `n` prefix (new file/folder), text editing |
| `render.rs` | `render_file_browser()` renders overlay with path bar, file list (icons, selection, scrolling), and a two-row options bar (filter + search + hidden state; contextual actions). Centered popups for the name input / rename input / delete confirmation / error, plus the Locations overlay |

## Contracts

- `FileBrowserState::new(mode, &FileBrowserSession)` starts at the session's last directory when it is still listable and falls back to the user's home directory otherwise. The session's hidden-entry setting is restored.
- `FileBrowserState::session()` captures `current_dir` and `show_hidden`; `vault::widget` stores the returned `FileBrowserSession` in `HomeState` and passes it to every subsequent opening. The session is in memory only — it is never written to config or disk.
- `handle_key(key, state) → Option<FileBrowserAction>` — returns `None` while browsing, `Some(Selected(path))` or `Some(Cancelled)` to exit.
- Enter in the name input returns `Selected` only when no file with that name exists at the target path; otherwise it shows a dismissable error popup and stays open.
- The name input, rename input, delete confirmation, and errors render as centered popups stacked over the browser; an error popup sits on top of any active dialog and is dismissed by the next key.
- The Locations overlay pins `Home` first, then lists mounted disks returned by `file_handling::list_disks()`. It preselects the location with the longest path that is an ancestor of `current_dir`, falling back to the first row. Enter on an unreadable location keeps the overlay open and shows the error popup on top.
- Hidden entries are names starting with `.`; nothing else is treated as hidden. `list_dir` excludes them and `list_dir_including_hidden` includes them.
- The file list and Locations overlay keep a fixed 3-row scroll margin (`SCROLL_OFFSET`) around the selection, clamped on short viewports. At the end of the list the last item may sit on the bottom row, matching Vim/Neovim `scrolloff` behavior.
- All I/O (list_dir, list_disks, delete_file, rename, create_dir, probe writability) goes through `file_handling`.
- Re-exports `FileEntry` from `file_handling` so consumers don't need to import it separately.

## Keybindings

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate files |
| `Enter` | Enter directory or select file |
| `/` | Search filter |
| `D` | Open the Locations overlay (list focus only, not while searching) |
| `Ctrl+h` / `Ctrl+Backspace` | Toggle hidden (dot-prefixed) entries (list focus; ignored in search and text inputs) |
| `h` | Go up one directory (or Esc to cancel in CreateFile/ImportFile mode) |
| `r` | Rename focused entry |
| `d` / `Delete` | Delete with confirmation (`y` confirms) |
| `n` | Prefix: `nn` → focus name input, `nN` → new folder (CreateFile mode) |
| `N` | (CreateFile mode) second key of the `n` prefix → new folder |
| `f` | Toggle filter: all files / `.pwd` only |
| `Esc` | Cancel/close browser (in the name input: return to the list, keeping the draft; in the rename input: cancel the rename and discard edits, keeping the browser open; in the Locations overlay: close the overlay) |

### Locations overlay

| Keys | Action |
|------|--------|
| `j`/`k`, `Down`/`Up` | Move between locations (wraps) |
| `Enter`, `l`, `Right` | Open the selected location |
| `Esc`, `h`, `Left`, `D` | Close the overlay |

## Dependencies

- `file_handling` — list_dir / list_dir_including_hidden, list_disks, delete, rename, create_dir, probes
- `theming` — styles, strings
- `cursor` — text editing in inputs