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
| `state.rs` | `FileBrowserState`, `FileBrowserMode` (OpenFile/CreateFile/ImportFile/ExportFile{format}/ShareDestination), `FileBrowserSession` (last directory + hidden setting), `Location` (Home or mounted disk row), `FocusTarget` (List/Input/Rename/Locations), `AddPrefix` (None/Pending), `FileFilter` (All/PwdKeeperOnly), `FileBrowserAction` (Selected/Cancelled/Renamed), viewport offsets (`scroll`, `location_scroll`) |
| `handler.rs` | `handle_key()` dispatches to list/input/rename/locations handlers. Supports j/k navigation, Enter, search, filter toggle, hidden-entry toggle (`Ctrl+h`), directory traversal, rename, delete, the `a` prefix (add file/directory), text editing. Rename and delete are offered only in the modes that may edit the filesystem (`OpenFile`, `CreateFile`) |
| `render.rs` | `render_file_browser()` renders the overlay with the path bar and file list (icons, selection, scrolling). It draws no status bar of its own — the owning Home window renders the browser's badge and hints from `status_parts()`, the mode→`(badge, style, hint)` mapping. Centered popups for the name input / rename input / overwrite confirmation / delete confirmation / error, plus the Locations overlay |

## Contracts

- `FileBrowserState::new(mode, &FileBrowserSession)` starts at the session's last directory when it is still listable and falls back to the user's home directory otherwise. The session's hidden-entry setting is restored.
- `FileBrowserState::for_export(format, base_path, node_name, &FileBrowserSession)` starts on the file list instead, in the secret file's directory, with `file_handling::suggest_export_name` prefilled and shown in the path bar (resolved to the format's extension). See [ADR 0022](../../../../docs/adr/0022-user-chosen-export-destination.md).
- `FileBrowserState::for_share(base_path, node_name, &FileBrowserSession)` names the folder a share bundle is written into: it also starts in the secret file's directory, with `file_handling::suggest_share_name` prefilled. See [ADR 0023](../../../../docs/adr/0023-sharing-a-vault-subtree.md).
- `FileBrowserState::session()` captures `current_dir` and `show_hidden`; `vault::widget` stores the returned `FileBrowserSession` in `HomeState` and passes it to every subsequent opening. The session is in memory only — it is never written to config or disk.
- `handle_key(key, state) → Option<FileBrowserAction>` — returns `None` while browsing, `Some(Selected(path))` or `Some(Cancelled)` to exit.
- Enter in the name input returns `Selected` only when no file with that name exists at the target path; otherwise it shows a dismissable error popup and stays open. In `ExportFile` mode Enter only accepts the draft and returns to the list; `e` on the list resolves the typed name through `file_handling::output_path`, refusing a blank name and a secret-file target and asking for an overwrite confirmation (`confirm_overwrite`, dismissed by any other key) before reporting `Selected` for the directory now open.
- In `ShareDestination` mode `e` on the list (like an export's write key) and Enter in the name input both commit the typed folder: a blank name is refused, a path that exists as a file is refused, and an existing folder that is not empty asks for confirmation (`confirm_overwrite`) before reporting `Selected` for the folder. The list only picks the parent directory.
- The name input, rename input, delete confirmation, and errors render as centered popups stacked over the browser; an error popup sits on top of any active dialog and is dismissed by the next key.
- The Locations overlay pins `Home` first, then lists mounted disks returned by `file_handling::list_disks()`. It preselects the location with the longest path that is an ancestor of `current_dir`, falling back to the first row. Enter on an unreadable location keeps the overlay open and shows the error popup on top.
- Hidden entries are names starting with `.`; nothing else is treated as hidden. `list_dir` excludes them and `list_dir_including_hidden` includes them.
- The file list and Locations overlay track viewport offsets (`scroll`, `location_scroll`) and keep a fixed 3-row margin (`SCROLL_OFFSET`) around the selection. The cursor moves inside that margin before the viewport scrolls, in both directions; the margin is clamped on short viewports, and at the end of the list the last item may sit on the bottom row, matching Vim/Neovim `scrolloff` behavior.
- `render_file_browser()` takes `&mut FileBrowserState` because it maintains those viewport offsets while rendering.
- All I/O (list_dir, list_disks, delete_file, rename, create_dir, probe writability) goes through `file_handling`.
- Re-exports `FileEntry` from `file_handling` so consumers don't need to import it separately.

## Keybindings

| Keys | Action |
|------|--------|
| `j`/`k` | Navigate files |
| `Enter` | Enter directory or select file (in `ExportFile` and `ShareDestination` modes, a file row does nothing: only the open directory is a target) |
| `n` | (`ExportFile`/`ShareDestination` mode) Focus the name input; in `ExportFile` Enter accepts the draft and returns to the list, in `ShareDestination` Enter commits the bundle folder |
| `e` | Write or commit the destination named in the path bar: (`ExportFile` mode) write the export (resolves the format's extension, refuses a secret-file or blank name, asks before overwriting); (`ShareDestination` mode) commit the typed folder the bundle is written into (refuses a blank name or an existing file, asks before writing into a non-empty folder) |
| `/` | Search filter |
| `D` | Open the Locations overlay (list focus only, not while searching) |
| `Ctrl+h` / `Ctrl+Backspace` | Toggle hidden (dot-prefixed) entries (list focus; ignored in search and text inputs) |
| `h` | Go up one directory (or Esc to cancel in CreateFile/ImportFile mode) |
| `r` | Rename focused entry |
| `d` / `Delete` | Delete with confirmation (`y` confirms) |
| `a` | Prefix: `af` → focus name input (add file), `ad` → focus name input (add directory) (CreateFile mode) |
| `f` / `d` | (CreateFile mode) second keys of the `a` prefix: `f` → add file, `d` → add directory |
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