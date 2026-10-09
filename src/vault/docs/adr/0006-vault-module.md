# 0006 — vault module

The vault module manages the Home screen, active vault lifecycle, and
configuration persistence. It bridges the Home and Tree screens through
`ActiveVaultState`.

**Decision:** the vault module owns the home screen event loop, modals, and
vault lifecycle. The tree module only knows about its own screen — it receives
a fully-loaded `Node` + path + passphrase from vault and runs independently.
When the tree returns to home, the vault re-opens the tree from the same
`ActiveVaultState` (re-decrypting if needed).

**Rejected alternative:** a single monolithic event loop covering both screens.
Would double the event-loop complexity and make screen transitions harder
to reason about.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `active_file.rs` | `ActiveVaultState` — holds `path: Option<String>`, `passphrase: Zeroizing<Vec<u8>>`, `node: Option<Node>`. Bridge between home and tree. |
| `widget.rs` | `HomeState`, `HomeModal` (11 variants), `run()` event loop, key handlers, all modal rendering (file browser, passphrase input, config, alerts, delete confirm, change password, export) |
| `utils.rs` | `Configuration` struct, `config_dir()`, `push_recent_secret_file()`, `save_config()`/`load_config()` via rkyv, legacy `settings.pwd` → `config.pwd` migration |

## Home screen flow

1. `cargo run` → `vault::widget::run()`
2. Menu: "Open secret file", "Create new secret file", "Import from file", recent files list, "Configuration", "Export active file", "Change master password", "Quit"
3. "Open secret file" → `FileBrowser` modal (OpenFile mode) → validate age header + size → `PassphraseInput` modal → decrypt → `push_recent_secret_file()` → launch tree
4. "Create new secret file" → `FileBrowser` (CreateFile mode) → probe writability → `PassphraseInput` (or `ConfirmNoPassphrase` for empty) → create encrypted file with root `"Secrets"` → launch tree
5. Auto-open on startup: if config has `open_last_opened_file_on_startup`, validates the file and opens PassphraseInput immediately
6. After successful open, path saved to recent secret files (deduplicated, most recent first)
7. Active vault path shown in window title bar: `" Home — /path/to/file "` / `" Tree — /path/to/file "`

## HomeModal variants

11 variants: `FileBrowser`, `PassphraseInput{file_path, file_data, passphrase, cursor, error}`,
`Configuration{setting_idx}`, `Alert{message}`, `DeleteConfirm{path}`,
`ChangePwConfirm`, `ConfirmNoPassphrase{file_path}`,
`ChangePwInput{current, new_pass, confirm, focused, error}`,
`ExportSelectType{path}`, `ExportConfirm{path, format}`, `ExportDone{message}`.

## Configuration

- Stored at `~/.config/cretopass/config.pwd` (XDG_CONFIG_HOME respected, `CRETOPASS_CONFIG_DIR` override).
- Serialized with `rkyv` (zero-copy binary format). Different strategy from JSON+age used for vault data — config is local-only, never encrypted, never exported.
- `Configuration` fields: `recent_files: Vec<String>`, `last_opened_file: Option<String>`, `display_full_path: bool`, `open_last_opened_file_on_startup: bool`.
- Legacy migration: reads old `settings.pwd` format, writes config in new format.

## Quirks & edge cases

- Quit sets `state.quit = true` → event loop breaks → `main()` cleanup (LeaveAlternateScreen, disable_raw_mode, show_cursor).
- `q` in tree quits everything (including home).
- `H` (shift+h) in tree returns to home screen.
- Wrong master password in PassphraseInput shows a red error bar; modal stays open.

## Dependencies

- `rkyv` — config serialization
- `zeroize` — passphrase zeroization
- `file_handling` — all I/O
- `file_browser` — directory browser overlay
- `theming` — Action dispatch, strings, styles
- `cursor` — text editing in modals
- `tree::node` — Node type for ActiveVaultState
- `tree::app_tree` — tree screen launch