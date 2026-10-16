# cretopass

A terminal-based secret keeper built with Rust and [ratatui](https://github.com/ratatui/ratatui).
It stores your secrets — logins, passwords, notes, and custom fields — in a
hierarchical tree, and keeps everything in **one encrypted file that never
leaves your machine**.

## Installation

### Linux & macOS

```sh
curl -fsSL https://github.com/Dmitrytuk/cretopass/releases/latest/download/install.sh | sh
```

The script downloads the latest release, verifies its checksum, installs the
binary to `~/.local/bin`, and sets up shell completions and desktop
integration. Re-running it is safe: if the installed version is already
current it exits early (pass `--force` to reinstall).

### Windows

```powershell
irm https://github.com/Dmitrytuk/cretopass/releases/latest/download/install.ps1 | iex
```

The script installs the binary to `%LOCALAPPDATA%\Programs\cretopass` and adds
it to `PATH`. Pass `-Force` to reinstall.

### Updating & uninstalling

```sh
cretopass update   # check for a new version and install it
cretopass remove   # delete the app and all its artifacts
```

## Core functionality

- **Secret tree** — organize secrets as a tree of nodes. Every node has a
  name, a description, and optional extra fields:
  - `Login`
  - `Password` (masked as `*****` unless actively edited)
  - `Custom` fields with arbitrary names
- **Vim-style navigation** — `j`/`k` to move, `h`/`l` to collapse/expand,
  `Enter` to toggle, `/` to search.
- **Search & filter** — live search with highlights; ancestors of matches
  auto-expand; `Enter` keeps highlights while freeing navigation.
- **Cut / copy / paste** — cut a node with `d d`, paste as sibling (`p`) or
  as child (`P`). The internal clipboard supports pasting multiple times.
- **Add & edit forms** — in-place forms for creating and editing nodes, with
  full cursor movement (`Left`/`Right`/`Home`/`End`) and extra-field
  management (add via the `+ Add Field` row, delete via `Del`).
- **Detail overlay** — view a node's fields (`Ctrl+Enter`/`i`), copy any row
  to the system clipboard with `Enter` (uses `wl-copy`/`xclip`/`xsel` on
  Linux, `pbcopy` on macOS).
- **Home screen** — the app starts with a launcher screen: open an existing
  secret file, create a new one, or pick from recently used files.
- **Auto-save** — every mutation (add, edit, cut, paste) re-encrypts and
  writes the file immediately using an atomic temp-file rename, so a crash
  mid-save cannot corrupt the vault.
- **Import / export** — secrets can be imported from and exported to
  JSON/CSV/XML files via the file browser.

## Security model

cretopass deliberately keeps its attack surface small by design:

- **One file, your place.** All secrets live in a single encrypted file that
  *you* choose the location of — your home directory, an encrypted USB
  drive, or your own sync folder. There is no cloud service, no account, no
  third-party server, and no telemetry. Nothing is ever transmitted over the
  network; the app does not use the network at all.
- **age encryption.** Files are encrypted with
  [age](https://age-encryption.org) using a passphrase you provide
  (scrypt-based key derivation). Without the passphrase the file is
  ciphertext.
- **In-memory editing.** Secrets are decrypted only into process memory.
  Sensitive buffers are zeroized (`zeroize`) when released. Password values
  are masked in the UI except in the field being actively edited.
- **Atomic writes.** On every save the new ciphertext is written to a
  temporary file, synced, then atomically renamed over the target, so a
  crash mid-save never leaves a truncated vault. A transient `{file}.bak`
  copy of the previous version exists only during the write and is removed
  once the save succeeds.
- **Clipboard is opt-in and explicit.** Data reaches the system clipboard
  only when you press `Enter` in the detail overlay — never implicitly. Be
  aware that anything placed on the system clipboard may be readable by
  other applications until overwritten.

**You are responsible for the passphrase.** There is no recovery mechanism:
if you forget it, the data cannot be decrypted. Keep your own backups of the
secret file — they are useless to anyone without the passphrase.

## Data & configuration

| Path | Purpose |
|------|---------|
| `<secret file>` | The age-encrypted secret file (location chosen by you) |
| `<secret file>.bak` | Transient copy of the previous version; exists only during a save (or after an interrupted one) |
| `~/.config/cretopass/config.pwd` | App settings and recent files list (rkyv binary) |

## License

MIT — see [LICENSE](LICENSE).
Third-party crate licenses — see [LICENSE-THIRD-PARTY](LICENSE-THIRD-PARTY).
