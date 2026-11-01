# 0001 — I/O gateway (`file_handling`)

The `file_handling` module is the **single point of filesystem access** for the
entire application. All vault and document I/O is mediated through this module
so that encryption, error handling, and import/export/share logic share one
place.

**Deliberate exception:** the CLI lifecycle commands (`cli/layout.rs`,
`cli/remove.rs`, `cli/update.rs`) still use `std::fs` directly. They manage the
app's own install artifacts rather than secrets, and are kept independent of the
gateway so `remove`/`update` do not depend on the vault stack. Every other
module — including the Home config in `vault/utils.rs` — writes only through
this gateway.

**Rejected alternative:** sprinkling `std::fs` calls across modules. This would
make it impossible to guarantee atomic saves, enforce encryption on write, or
audit I/O patterns.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `atomic.rs` | the atomic write layer: an exclusively created temp file + `fsync` + `rename` (replacing), no-clobber `commit_new`, symlink-safe `open_for_read`, owner-only modes, best-effort directory `fsync`, and the per-platform durability statement |
| `file_process.rs` | JSON serde, raw read/write, encrypted file ops, directory listing (visible or including hidden), path probes, age-header detection, owner-only permissions; every write is delegated to `atomic.rs` |
| `autosave.rs` | `Autosave`: one background writer thread with a latest-wins snapshot slot, generation numbers, `SaveEvent` notifications, panic containment, and flush-on-drop ([ADR 0010](../../../../docs/adr/0010-async-tree-autosave.md)) |
| `disks.rs` | `DiskInfo` + `list_disks()`: cross-platform mounted-volume enumeration via `sysinfo`, with loop/squashfs filtering |
| `error.rs` | `FileHandlingError` enum (+ `crypt::CryptError` for typed encryption/decryption failures) wrapping Io / Serde / typed crypt errors + structured domain errors; `#[non_exhaustive]` and free of any imported-value payload |
| `export.rs` | `ExportFormat` (an alias of `format::Format`), recursive XML serialization, flattened CSV export |
| `format.rs` | the shared `Format` enum behind the `ImportFormat` / `ExportFormat` aliases: one extension table for detection and serialisation |
| `import.rs` | `ImportFormat` detection (an alias of `format::Format`), JSON/XML/CSV import with custom parsers, `build_tree_from_paths()` |
| `share.rs` | `suggest_share_name` (the bundle-folder name) and `share_tree`: writes a share bundle — a re-encrypted secret file via `save_tree`, then a `HOW-TO-OPEN.txt` instruction ([0023](../../../../docs/adr/0023-sharing-a-vault-subtree.md)) |

## Facade

The submodules are `pub(crate)`. Every caller imports the symbols it needs from
`file_handling::…` directly; the `pub use` list in `src/file_handling.rs` is the
single facade, so a submodule can be renamed or split without touching any call
site outside this module.

## Contracts

- All public functions accept `&str` or `&Path` for paths (not `PathBuf`).
- `save_tree()` is crash-safe: it writes the ciphertext to an **exclusive**
  temp file (`O_CREAT | O_EXCL`, a random name, mode `0600`) in the target's
  directory, `fsync`s it, `rename`s it over the target, then best-effort
  `fsync`s the directory. The exclusive random name means a planted symlink is
  never followed and two concurrent writers never collide. There is no `.bak`
  file: recovery is the atomic rename itself.
- Replacing an existing **regular** file preserves that file's mode; a mode is
  never inherited from a symlink's target. Files the gateway creates for
  secrets are `0600` and secret directories are `0700` on Unix. A plaintext
  export is always written `0600`, never inheriting a more permissive mode.
- `create_secret_file()` creates the vault with a single **no-clobber rename**
  (`renameat2(RENAME_NOREPLACE)` / `renamex_np`, falling back to `link` +
  `unlink`), so an existing file (or a symlink) is never overwritten and no
  hard kill can leave a zero-byte placeholder.
- Reads are symlink-safe: `validate_encrypted_file()`, `import_tree()`,
  `read_file()` and the age-header scan open the file with `O_NOFOLLOW`
  (Linux/macOS) and reject anything that is not a regular file, so a planted
  symlink is never followed. A vault kept *behind* a symlink is therefore
  deliberately refused rather than followed. This applies to the Home config
  too: `read_file()` is how `vault/utils.rs` loads `config.pwd`, so a
  dotfiles-style symlink there does not load: the config layer substitutes
  defaults and writes them back at once, replacing the link with a regular
  file. That consequence is intentional — one symlink policy for every
  file-content read through the gateway — and is noted in
  [0006](../../../vault/docs/adr/0006-vault-module.md).
- Durability is stated per platform in
  [`atomic.rs`](../../atomic.rs): Linux/macOS `fsync` both the
  file and its directory; Windows cannot `fsync` a directory with `std`, so the
  rename is best-effort. In every case a reader sees either the previous file or
  the complete new one — never a torn write.
- `share_tree()` refuses a destination that already exists as a file, creates the folder, writes the bundle's secret file through `save_tree()`, and only then writes the `HOW-TO-OPEN.txt` instruction.
- `Autosave` serializes all writes through one thread. Callers submit a root snapshot and receive a generation; a newer submission replaces one that has not started. The worker emits `SaveEvent` after every attempt. Because `save_tree()` uses an exclusive random temp name, concurrent saves for the same path cannot collide.
- `validate_encrypted_file()` enforces a 50MB size cap and scans for the `age-encryption.org` magic header. `import_tree()` shares the same cap.
- `list_dir()` skips hidden (dot-prefixed) entries; `list_dir_including_hidden()` returns them too. Both sort directories first, then alphabetically.
- `list_disks()` returns project-owned `DiskInfo` rows sorted by mount point. `sysinfo` excludes pseudo filesystems, tmpfs, optical media, network shares, and loop-backed `squashfs` mounts; a defensive filter also drops `/dev/loop*` and snap mounts. Returns an empty vector when no disks are reported.
- Import parsers (XML, CSV) are hand-rolled to avoid pulling in parser crates. Max XML nesting depth is 256 and max CSV path depth is also 256.
- `FileEntry` is the public type for directory listings; `file_browser` re-exports it.
- `DiskInfo` is the public type for mounted disks; `file_browser` consumes it via `list_disks()`.

For file_browser keybindings see [ADR 0007 — file_browser](../../file_browser/docs/adr/0007-file-browser.md).

## Dependencies

- `age` 0.11 — encryption
- `serde` 1 + `serde_json` 1 — tree serialization
- `sysinfo` 0.38 (`disk` feature only) — cross-platform mounted-disk enumeration
- `tempfile` 3 — the atomic layer's exclusively created temp file and its
  `persist` / `persist_noclobber` rename (`atomic.rs`)
- `std` only for autosave — `std::thread`, `Mutex`, `Condvar`; no async runtime in this module