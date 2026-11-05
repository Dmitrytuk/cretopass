# 0000 — Architecture overview

cretopass is a Rust ratatui-based TUI password manager with two screens (Home → Tree),
age-based encryption, and a centralized I/O gateway. All data lives in memory; tree
mutations are submitted to a background autosave worker, which writes the latest
snapshot back to an age-encrypted JSON file.

## Module map

| Module | ADR | Role |
|--------|-----|------|
| `file_handling` | [0001](../../src/file_handling/docs/adr/0001-io-gateway.md) | I/O gateway — the single vault/document I/O boundary (only the CLI lifecycle commands manage their own install artifacts); background autosave worker ([0010](0010-async-tree-autosave.md)) and background directory/disk loaders ([0024](0024-browsing-never-reads-file-contents.md)) |
| `crypt` | [0002](../../src/crypt/docs/adr/0002-crypt.md) | age-based encryption/decryption |
| `cursor` | [0003](../../src/cursor/docs/adr/0003-cursor.md) | UTF-8 byte-offset cursor editing + TUI span helpers, including the masked password fields (`SecretField`/`SecretFieldGroup`) |
| `theming` | [0004](../../src/theming/docs/adr/0004-theming.md) | Centralized keybindings, styles, icons, UI strings, the switchable [dark and light colour schemes](0019-runtime-colour-scheme-switching.md) ([0015](0015-default-dark-colour-scheme.md)), the title-bar `Status`/`Spinner` ([0011](0011-animated-status-spinner.md)), and the shared status bar ([0014](0014-status-bar-unification.md)) |
| `generator` | [0012](0012-password-generator.md) | Password generation — strict leaf, depends only on `rand`; TUI-free and tree-free |
| `share` | [0023](0023-sharing-a-vault-subtree.md) | Share flow, UI-agnostic: the recipient's masked password pair (`Passphrase`), wrapped by the Tree and Home modals |
| `tree` | [0005](../../src/tree/docs/adr/0005-tree-module.md) | Tree data model, state machine, rendering, async event loop |
| `vault` | [0006](../../src/vault/docs/adr/0006-vault-module.md) | Home screen, active vault state, configuration persistence |
| `file_browser` | [0007](../../src/file_browser/docs/adr/0007-file-browser.md) | OS directory browser overlay with mounted-disk locations and hidden-entry toggle; listings and disk enumeration run off the UI thread, and browsing reads no file contents ([0024](0024-browsing-never-reads-file-contents.md)) |
| `cli` | [0009](../../src/cli/docs/adr/0009-cli-module.md) | CLI lifecycle subcommands (`update`, `remove`) — no TUI |

## Cross-module data flow

```
main.rs  ──▶  lib.rs  ──▶  vault::widget::run()   ──▶  tree::app_tree::run_app()
                  │               │                            │
                  │          file_browser             submit snapshot
                  │               │                            │
                  │          file_handling  ◀──────────  autosave worker
                  │               │                     (latest wins)
                  │          crypt (decrypt on open,
                  │                 encrypt on save)
                  │
              theming  ◀──  cursor
            (used by all UI)
```

The `generator` module sits below `tree`: `tree::app_tree` maps a `Password`
field to `generator::generate_password` when the user runs the generator.
`generator` never imports `tree` (or any other project module).

The `share` module sits beside the two screens: it owns the recipient's masked
password pair that the Tree and Home share flows both use, and never imports
`TreeState`/`HomeState`. Writing the bundle itself is
`file_handling::share`'s job ([0023](0023-sharing-a-vault-subtree.md)).

## Core design principles

1. **I/O gateway** — all vault and document I/O goes through `file_handling`. Every write is atomic (exclusive temp file + `fsync` + rename) and reads are symlink-safe. The CLI lifecycle commands are the single deliberate exception, managing their own install artifacts with `std::fs` (see [ADR 0001](../../src/file_handling/docs/adr/0001-io-gateway.md)).
2. **State-machine UI** — every screen and modal is an enum variant in `InputMode` (tree) or `HomeModal` (vault), carrying its own transient state. Key handlers dispatch by variant.
3. **DFS flat-list rendering** — the tree view flattens the recursive `Node` tree into a `Vec<VisibleNode>` with indentation. Expansion state is a `HashSet<Uuid>`.
4. **Asynchronous atomic autosave** — mutations submit a root snapshot to a single background writer. The worker serializes, encrypts, and writes via an exclusive temp file + fsync + rename + directory fsync. A newer snapshot replaces a waiting one; `q`/`H` flush the latest generation before leaving.
5. **Theming as a strict leaf** — all UI strings, styles, icons, and keybindings live in `theming/` and depend on nothing else in the project.
6. **byte-offset cursor** — all text editing uses byte offsets with explicit UTF-8 boundary checks via `is_char_boundary()` and `len_utf8()`.
7. **Generator as a strict leaf** — password generation is a pure, TUI-free module. The tree maps a field type to a generator; the module never imports `FieldType` or any TUI code (see [0012](0012-password-generator.md)).
8. **Never block the UI on I/O** — directory listings and disk enumeration run on detached background threads with a bounded wait, and browsing reads no file contents, so a slow or hung filesystem (a Windows cloud-sync folder, a disconnected network mount) cannot freeze the TUI (see [0024](0024-browsing-never-reads-file-contents.md)).

## Dependencies

| Crate | Used by | Purpose |
|-------|---------|---------|
| `ratatui` 0.28 | tree, vault, file_browser, cursor, theming | TUI rendering |
| `crossterm` 0.28 (`event-stream`) | lib, tree, vault | terminal events, raw mode, async event stream |
| `tokio` 1 (`rt`, `sync`, `macros`, `time`) | tree | current-thread runtime, `select!`, save-event channel, spinner `interval` and bounded `timeout` |
| `futures-util` 0.3 | tree | `StreamExt::next` for `EventStream` |
| `age` 0.11 | crypt | passphrase-based encryption |
| `rand` 0.8 | generator | cryptographically secure password randomness (already in the graph via `age`) |
| `serde` / `serde_json` 1 | file_handling, node | JSON serialization of tree |
| `uuid` 1 (v4, serde) | node, widget | stable node identities |
| `rkyv` | vault::utils | zero-copy config serialization |
| `zeroize` | vault::active_file, vault::widget, tree | secure passphrase wipe on drop |
| `sysinfo` 0.38 (disk) | file_handling::disks | cross-platform mounted-disk enumeration for file-browser Locations |
| `tempfile` 3 | file_handling::atomic | exclusively created temp file + no-clobber `persist` for every atomic write |

## Re-exports (public API)

`src/lib.rs` re-exports for external consumers: `Node`, `ExtraField`, `FieldType`,
`InputMode`, `TreeState`, `TreeWidget`, `run()`, `run_with_root()`.