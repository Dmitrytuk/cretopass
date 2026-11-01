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
| `widget.rs` | The Home screen: `run()` event loop, modal dispatch, `run_with_spinner`. Re-exports `HomeState`. |
| `widget/state.rs` | `HomeState` — menu cursor, active vault, busy status and spinner, File Browser session memory, generator settings. |
| `widget/menu.rs` | The Home menu: `handle_home_key` and the pure `menu_intent`. |
| `widget/modal.rs` | The contract vocabulary: `Modal` (the one-window slot) and `HomeAction` (what a modal asks for); re-exports the modal types. |
| `widget/modal/` | One file per modal: `browser.rs`, `unlock.rs`, `create_secret.rs`, `change_password.rs`, `configuration.rs`, `confirmation.rs`, `export.rs`, `share.rs`, `alert.rs`, `about.rs`. |
| `widget/orchestrator.rs` | The one place that performs a `HomeAction`: `execute`, plus the File Browser openers. |
| `widget/orchestrator/` | One file per flow group: `vault.rs` (unlock, create, re-key, launch Tree), `import.rs`, `recent.rs`, `settings.rs`, `export.rs`, `share.rs`. |
| `widget/render.rs` | Rendering the menu, the modal on top of it, and the status bar. |
| `widget/chrome.rs` | Overlay geometry and painting shared by the modals. |
| `active_file.rs` | `ActiveVaultState` — holds `path: Option<String>`, `passphrase: Zeroizing<Vec<u8>>`, `node: Option<Node>`. Bridge between home and tree. |
| `utils.rs` | `Configuration` struct, `config_dir()`, `push_recent_secret_file()`, `save_config()`/`load_config()` via rkyv, legacy `settings.pwd` → `config.pwd` migration |

Modals never do I/O and never build one another: they report a `HomeAction` and
the orchestrator performs it. See [0021](0021-home-intent-contract.md).

## Home screen flow

1. `cargo run` → `vault::widget::run()`
2. Menu: "Open secret file", "Create new secret file", "Configuration", "Change file password", "Export active file", "Share active file", "Import from file", then the recent files list
3. "Open secret file" → `BrowserModal` (OpenFile mode) → validate age header + size → `UnlockPrompt` → decrypt → `push_recent_secret_file()` → launch tree
4. "Create new secret file" → `BrowserModal` (CreateFile mode) → Enter on a name hard-blocks if the path already exists → probe writability → `CreateSecretPrompt` (passphrase + confirmation, `Ctrl+S` reveals) → create encrypted file with root `"Secrets"` → launch tree. An all-empty pair falls through to the `Confirmation` asking for a passwordless file.
5. Auto-open on startup: if config has `open_last_opened_file_on_startup`, validates the file and opens the unlock prompt immediately
6. After successful open, path saved to recent secret files (deduplicated, most recent first)
7. Active vault path shown in window title bar: `" Home — /path/to/file "` / `" Tree — /path/to/file "`

## Modals

Ten `Modal` variants: `FileBrowser`, `Unlock`, `CreateSecret`, `Configuration`,
`Alert`, `Confirmation` (carrying a `ConfirmKind`), `ChangePassword`, `Export`
(a one-step `ExportWizard` picking the format, with the destination then named in
the File Browser), `SharePassphrase` (the recipient's password and confirmation
for a share bundle), `About`.

Password fields (`Unlock`, `ChangePassword`, `CreateSecret`) mask input by
default; `Ctrl+S` toggles plaintext. `CreateSecret` validates that the passphrase
and its confirmation match (`MSG_PW_MISMATCH`) before writing. A successful create
(`CreateSecret` or the passwordless `Confirmation`) clears the modal before
launching the tree, so returning to home does not re-show the dialog.

## Configuration

- Stored at `~/.config/cretopass/config.pwd` (XDG_CONFIG_HOME respected, `CRETOPASS_CONFIG_DIR` override).
- Serialized with `rkyv` (zero-copy binary format). Different strategy from JSON+age used for vault data — config is local-only, never encrypted, never exported.
- `Configuration` fields: `recent_files: Vec<String>`, `last_opened_file: Option<String>`, `display_full_path: bool`, `open_last_opened_file_on_startup: bool`, `palette_name: String` (active colour scheme, applied on startup; see [0019](../../../docs/adr/0019-runtime-colour-scheme-switching.md)).
- Legacy migration: reads old `settings.pwd` format, writes config in new format.
- The config is read and written through `file_handling`, so it inherits the gateway's symlink-refusing reads ([0001](../../../file_handling/docs/adr/0001-io-gateway.md)): a `config.pwd` kept behind a symlink (a dotfiles-manager pattern) does not load. `load_config_from` then substitutes defaults and writes them straight back, so — when that best-effort write-back succeeds — the link is replaced by a regular file and the settings are silently lost.

## Quirks & edge cases

- Quit sets `state.quit = true` → event loop breaks → `main()` cleanup (LeaveAlternateScreen, disable_raw_mode, show_cursor).
- `q` in tree quits everything (including home).
- `H` (shift+h) in tree returns to home screen.
- Wrong master password in the unlock prompt shows a red error bar; the prompt stays open with the typed text.
- Creating a file that already exists is refused before any password prompt (hard block, `MSG_FILE_EXISTS` alert); the file browser also shows an inline error when Enter is pressed on an existing name.
- An alert is dismissed by any key, as its hint promises; the About card likewise.
- `j`/`k` are not bound to field navigation while editing passwords, so they can be typed as password characters.

## Dependencies

- `rkyv` — config serialization
- `zeroize` — passphrase zeroization
- `file_handling` — all I/O
- `file_browser` — the directory browser, wrapped as a `Modal` variant
- `theming` — Action dispatch, strings, styles
- `cursor` — the masked password fields (`SecretField`) and text editing
- `share` — the UI-agnostic share flow: the scope vocabulary and the recipient's masked password pair, shared with the Tree
- `tree::node` — Node type for ActiveVaultState
- `tree::app_tree` — tree screen launch
