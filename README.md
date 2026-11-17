# cretopass

A tree of secrets in the terminal. One file. No network.

Offline terminal password manager, written in Rust. A TUI over one age-encrypted file — not a browser vault and not a `pass` store.

I use it when I need an account I have not touched in months: open the tree, search, copy a field. A node is a name plus whatever fields I need — login, password, a note, a token — not a password slot with a title.

![Opening a recent file, searching, copying a field, and adding a node with a non-password field](docs/demo.gif)

- Tree. Search, copy a field, generate one when creating the account.
- One age-encrypted file. I pick the path. Backup is a copy of that file. A forgotten passphrase is not recoverable.
- No network. The binary makes no network calls. Update was removed on purpose ([ADR 0027](docs/adr/0027-no-in-app-network-access.md)).

```sh
cargo install cretopass
```

No Rust toolchain: the release binary and its checksum are on the [latest release](https://github.com/Dmitrytuk/cretopass/releases/latest).

What it does not do:

- no sync and no shared account
- no browser autofill, no mobile app
- no `cretopass get` for scripts
- no independent audit

| | cretopass | pass | gopass | KeePassXC |
|---|---|---|---|---|
| Where it lives | terminal, a tree | commands, flat files | commands | Qt |
| Encryption | age, passphrase | GPG | GPG and age | AES-KDBX |
| Besides a password | any fields on the node | a file is the secret | partial | yes |
| Network and account | none | none | none; git optional | none |
| Browser, sync, phone | none | none | git, not a browser | yes, that is its strength |
| Install today | cargo or a release binary | packages everywhere | packages everywhere | packages everywhere |

For people who find `pass` and `gopass` too much GPG and too many commands, and KeePassXC a different window plus a browser. This does not read the `pass` format.

## Installation

The commands below are the no-toolchain installer. They are not the first step: `cargo install cretopass` is.

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
cretopass remove   # delete the app and all its artifacts
```

The app never connects to the network, so it does not update itself. To update,
re-run the installer (the same command you installed with) or use your package
manager:

```sh
# Linux & macOS
curl -fsSL https://github.com/Dmitrytuk/cretopass/releases/latest/download/install.sh | sh
```

```powershell
# Windows
irm https://github.com/Dmitrytuk/cretopass/releases/latest/download/install.ps1 | iex
```

## Core functionality

- **Secret tree** — organize secrets as a tree of nodes. Every node has a
  name and optional extra fields:
  - `Login`
  - `Password` (masked as `*****` unless actively edited)
  - `Custom` fields with arbitrary names, including the conventional
    `Description` field
- **Vim-style navigation** — `j`/`k` to move, `h`/`l` to collapse/expand,
  `Enter` to toggle, `/` to search.
- **Search & filter** — live search with highlights; ancestors of matches
  auto-expand; `Enter` keeps highlights while freeing navigation.
- **Cut / copy / paste** — cut a node with `d d`, then paste with `p` (after),
  `P` (before), or `l`/`→` (as a child). The internal clipboard supports
  pasting multiple times.
- **Add & edit forms** — in-place forms for creating and editing nodes, with
  full cursor movement (`Left`/`Right`/`Home`/`End`) and extra-field
  management (add via the `+ Add Field` row, delete via `Del`).
- **Password & login generators** — `Ctrl+G` in a field opens a generator: for a
  `Password` field, a configurable password (length plus a special-character
  checklist); for a `Login` field, a pronounceable word. Generation coalesces
  into a single undo step.
- **Detail overlay** — view a node's fields (`Ctrl+Enter`/`i`) and copy any
  row to the system clipboard with `c`/`y`. Copying uses OS-native APIs
  (`arboard`) on Windows, macOS, and X11 Linux, falling back to
  `pbcopy`/`wl-copy`/`xclip`/`xsel` when needed, and shows the outcome as a
  result modal.
- **Clipboard paste** — paste text copied from another application into any text
  field with `Ctrl+V` (or the terminal's own paste). Multi-line text is joined
  into one line, and `Ctrl+V` never types the letter `v`.
- **Mouse support** — the wheel scrolls every list identically in every terminal,
  and a click selects a row (a repeat click activates it); a right-click on a
  detail row copies its value, and a click outside a modal closes it. Native
  terminal selection is off while the app runs; hold `Shift` to select text.
- **Home screen** — the app starts with a launcher screen: open an existing
  secret file, create a new one, change the active file's password, export or
  share it, or pick from recently used files.
- **Configuration** — a Home modal switches the colour scheme live (reverting on
  `Esc`) and persists your choice.
- **File browser** — names the file or folder for open/create/import/export/share.
  `D` opens a `Locations` list of mounted disks, and `Ctrl+h` toggles hidden
  dot-entries.
- **Auto-save** — every mutation (add, edit, cut, paste) re-encrypts and
  writes the file immediately using an atomic temp-file rename, so a crash
  mid-save cannot corrupt the vault.
- **Import / export** — secrets can be imported from and exported to
  JSON/CSV/XML files via the file browser.
- **Share bundles** — hand a node (with or without its children) or the whole
  vault to someone else as a still-encrypted folder they open with their own
  password, plus a plain-text `HOW-TO-OPEN.txt`; reached from the Tree (`Ctrl+E`,
  then `s`) or the Home menu's "Share active file".

## Security model

cretopass deliberately keeps its attack surface small by design:

- **One file, your place.** All secrets live in a single encrypted file that
  *you* choose the location of — your home directory, an encrypted USB
  drive, or your own sync folder. There is no cloud service, no account, no
  third-party server, and no telemetry. Nothing is ever transmitted over the
  network: the binary contains no network code at all — no telemetry, no
  account, no update check, and no self-update. Updating is an explicit action
  you take outside the app.
- **age encryption.** Files are encrypted with
  [age](https://age-encryption.org) using a passphrase you provide
  (scrypt-based key derivation). Without the passphrase the file is
  ciphertext.
- **In-memory editing.** Secrets are decrypted only into process memory.
  Sensitive buffers are zeroized (`zeroize`) when released. Password values
  are masked in the UI except in the field being actively edited.
- **Atomic writes.** On every save the new ciphertext is written to an
  exclusive temporary file (a random name, never following a planted
  symlink), synced, then atomically renamed over the target, and the
  containing directory is synced, so a crash mid-save never leaves a
  truncated vault. New secret files are created owner-only (`0600`), and a
  plaintext export is always written owner-only.
- **Clipboard is opt-in and explicit.** Data reaches the system clipboard
  only when you press `c`/`y` on a field in the detail overlay — never
  implicitly. If the clipboard is unavailable, a copy fails visibly with an
  error instead of pretending to succeed. Likewise, a `Ctrl+V` paste reads the
  clipboard only when you press it. Be aware that anything placed on
  the system clipboard may be readable by other applications until
  overwritten.

**You are responsible for the passphrase.** There is no recovery mechanism:
if you forget it, the data cannot be decrypted. Keep your own backups of the
secret file — they are useless to anyone without the passphrase.

## Data & configuration

| Path | Purpose |
|------|---------|
| `<secret file>` | The age-encrypted secret file (location chosen by you) |
| `~/.config/cretopass/config.pwd` | App settings and recent files list (rkyv binary) |

## Feedback & Security

Found a bug or have a feature request? The single place to report it is GitHub
Issues:

👉 https://github.com/Dmitrytuk/cretopass/issues

For security vulnerabilities, do **not** open a public issue — use GitHub's
private vulnerability reporting instead (see [SECURITY.md](SECURITY.md)):

👉 https://github.com/Dmitrytuk/cretopass/security/advisories/new

The same links, together with the app version, are available in the app itself:
press `?` on the Home screen or run `cretopass --help`.

## License

MIT — see [LICENSE](LICENSE).
Third-party crate licenses — see [LICENSE-THIRD-PARTY](LICENSE-THIRD-PARTY).
