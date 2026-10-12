# cretopass

Rust ratatui-based TUI app with a custom tree data structure (not file-system), vim-style navigation, search/filter, and edit/add forms. In-memory editing with atomic autosave to an age-encrypted file.

## Coding Conventions

Read the applicable doc before writing or modifying code in its area:

- Main coding rules at: `@docs/conventions/rust_coding_rules.md`
- Detailed rules how to use borrowing: `@docs/conventions/rust_ownership_rules.md`

## Workflow Rules

### Workflow
1. Do one logical task. Follow the `Coding conventions`.
2. If one logical task is completed, then run subagent "git-committer".
3. Print subagent output.

### Commit Workflow
- If subagent "git-committer" fails do not perform commits.

## Build & Run

```sh
cargo build                          # debug build
cargo run                            # launch TUI
cargo doc --open                     # generate and open API docs from doc comments
```

## Architecture

Architecture decisions are documented as ADRs. See the overview for the module
map and cross-module data flow.

- [0000 — Architecture overview](./docs/adr/0000-architecture-overview.md) — module map, data flow, design principles, dependencies
- [0001 — I/O gateway (`file_handling`)](./src/file_handling/docs/adr/0001-io-gateway.md) — filesystem access, atomic save, import/export
- [0002 — crypt](./src/crypt/docs/adr/0002-crypt.md) — age-based encryption/decryption
- [0003 — cursor](./src/cursor/docs/adr/0003-cursor.md) — UTF-8 byte-offset editing, TUI span helpers
- [0004 — theming](./src/theming/docs/adr/0004-theming.md) — keybinding dispatch, styles, icons, strings
- [0005 — tree](./src/tree/docs/adr/0005-tree-module.md) — Node model, TreeState, InputMode, event loop, rendering, autosave
- [0006 — vault](./src/vault/docs/adr/0006-vault-module.md) — Home screen, ActiveVaultState, configuration persistence
- [0007 — file_browser](./src/file_browser/docs/adr/0007-file-browser.md) — directory browser overlay
- [0008 — CLI lifecycle commands](./docs/adr/0008-cli-lifecycle-commands.md) — `update`/`remove` architecture, self-managed installs, detached helpers
- [0009 — CLI module](./src/cli/docs/adr/0009-cli-module.md) — `cli` module facade + sub-module layout (`detect`, `update`, `remove`)