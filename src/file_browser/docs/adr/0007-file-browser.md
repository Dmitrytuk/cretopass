# 0007 — file_browser module

An overlay directory browser used by the Home screen for file selection (open,
create, import). Renders a scrollable file list with path bar, search filter,
and contextual actions (rename, delete, new folder).

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
| `state.rs` | `FileBrowserState`, `FileBrowserMode` (OpenFile/CreateFile/ImportFile), `FocusTarget` (List/Input/Rename), `FileFilter` (All/PwdKeeperOnly), `FileBrowserAction` (Selected/Cancelled/Renamed) |
| `handler.rs` | `handle_key()` dispatches to list/input/rename handlers. Supports j/k navigation, Enter, search, filter toggle, directory traversal, rename, delete, new folder, text editing |
| `render.rs` | `render_file_browser()` renders overlay with path bar, file list (icons, selection, scrolling), options bar (filter + search hints), optional file input / rename input / delete confirmation / error bar |

## Contracts

- `FileBrowserState` receives `current_dir` and `FileBrowserMode` on construction.
- `handle_key(key, state) → Option<FileBrowserAction>` — returns `None` while browsing, `Some(Selected(path))` or `Some(Cancelled)` to exit.
- All I/O (list_dir, delete_file, rename, create_dir, probe writability) goes through `file_handling`.
- Re-exports `FileEntry` from `file_handling` so consumers don't need to import it separately.

## Keybindings

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate files |
| `Enter` | Enter directory or select file |
| `/` | Search filter |
| `h` | Go up one directory (or Esc to cancel in CreateFile/ImportFile mode) |
| `r` | Rename focused entry |
| `d` / `Delete` | Delete with confirmation (`y` confirms) |
| `n` | New folder (CreateFile mode) |
| `N` | Toggle focus between file list and path input |
| `f` | Toggle filter: all files / `.pwd` only |
| `Esc` | Cancel/close browser |

## Dependencies

- `file_handling` — list_dir, delete, rename, create_dir, probes
- `theming` — styles, strings
- `cursor` — text editing in inputs