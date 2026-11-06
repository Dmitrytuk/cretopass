# 0001 — I/O gateway (`file_handling`)

The `file_handling` module is the **single point of filesystem access** for the
entire application. All vault and document I/O is mediated through this module
so that encryption, error handling, and import/export/share logic share one
place.

**Deliberate exception:** the CLI lifecycle command (`cli/layout.rs`,
`cli/remove.rs`) still uses `std::fs` directly. It manages the
app's own install artifacts rather than secrets, and is kept independent of the
gateway so `remove` does not depend on the vault stack. Every other module —
including the Home config in `vault/utils.rs` — writes only through this
gateway.

**Rejected alternative:** sprinkling `std::fs` calls across modules. This would
make it impossible to guarantee atomic saves, enforce encryption on write, or
audit I/O patterns.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `atomic.rs` | the atomic write layer: an exclusively created temp file + `fsync` + `rename` (replacing), no-clobber `commit_new`, symlink-safe `open_for_read`, owner-only modes, best-effort directory `fsync`, and the per-platform durability statement |
| `file_process.rs` | JSON serde, raw read/write, encrypted file ops, a bounded config read (`read_file`, cap `MAX_CONFIG_SIZE`), directory listing (visible or including hidden), path probes, age-header detection, owner-only permissions; every write is delegated to `atomic.rs` |
| `autosave.rs` | `Autosave`: one background writer thread with a latest-wins snapshot slot, generation numbers, `SaveEvent` notifications, panic containment, and flush-on-drop ([ADR 0010](../../../../docs/adr/0010-async-tree-autosave.md)) |
| `disks.rs` | `DiskInfo` + `list_disks()`: cross-platform mounted-volume enumeration via `sysinfo`, with loop/squashfs filtering |
| `error.rs` | the composed `FileHandlingError` enum: it is built from the layer-local errors (`atomic::WriteError`, `import::ParseError`, `crypt::CryptError`) plus the gateway's own file-level outcomes (size caps, age-header checks, `AlreadyExists`, export/share refusals); `SerdeContext` tags each JSON failure with the operation that hit it; `#[non_exhaustive]` and free of any imported-value payload |
| `export.rs` | `ExportFormat` (an alias of `format::Format`), the naming policy (`suggest_export_name` / `output_path`, and the `sanitize` rule `suggest_export_name` shares with the share flow), and `export_tree()` — a dispatcher that hands each format to its owning module |
| `format.rs` | the shared `Format` enum behind the `ImportFormat` / `ExportFormat` aliases: one extension table for detection and serialisation |
| `import.rs` | `import_tree()` (an alias `ImportFormat` = `format::Format`), the `ParseError` vocabulary, JSON parsing, and the BOM strip; it dispatches XML and CSV to their owning modules |
| `xml.rs` | the XML interchange format, both directions: `node_to_xml` (writer + escape) and `import_xml` (reader + unescape), so escape and unescape cannot drift |
| `csv.rs` | the flattened CSV interchange format, both directions: `node_to_csv` (writer + `csv_escape` + path-segment encoding) and `import_csv` (reader + `parse_csv_records` + `build_tree_from_paths`) |
| `passphrase.rs` | `Passphrase`: the one bridge between the passphrase bytes the app carries and the `&str` `age` accepts, owning the non-UTF-8 check |
| `share.rs` | `suggest_share_name` (the bundle-folder name) and `share_tree`: writes a share bundle — a re-encrypted secret file via `save_tree`, then a `HOW-TO-OPEN.txt` instruction whose wording [`theming::texts`](../../../theming/texts.rs) owns ([0023](../../../../docs/adr/0023-sharing-a-vault-subtree.md)) |

## Facade

The submodules are `pub(crate)`. Callers outside this module import the symbols
they need from `file_handling::…`; the `pub use` list in `src/file_handling.rs`
is the single facade, so a submodule can be renamed or split without touching any
call site outside this module. A symbol no caller anywhere needs is not
re-exported; one that only other in-crate modules use is a `pub(crate) use` (for
example `node_to_json`, which `export.rs` reaches through the facade). Inside the
module a submodule may still reach a sibling directly through `super::` (for
example `share.rs` taking `sanitize` from `export.rs`), so "one facade" governs
the module's callers, not its siblings' own imports.

## Contracts

- Errors are layered. The atomic write layer reports `atomic::WriteError`
  (`Io` / `AlreadyExists(path)` / `NotWritable(path)` / `Abandoned`) and the
  import parsers report `import::ParseError`; both compose into
  `FileHandlingError` through `From`, so only the atomic layer ever inspects an
  `std::io::ErrorKind` (the no-clobber rename is the one place that must). The
  UI matches the domain variant (`AlreadyExists { .. }`), never an `ErrorKind`.
  A JSON failure is a single `Serde { context, source }` arm whose context is
  `Load` (a vault that decrypts but does not deserialise — the only "damaged
  secret"), `Save`, or `Export`.
- The I/O entry points take `impl AsRef<Path>` for paths, so a caller may pass
  a `&str`, `String`, `&Path` or `PathBuf` without converting and a non-UTF-8
  path is never lossily mangled. The pure classifiers and naming helpers
  (`Format::from_path`, `output_path`, `suggest_export_name`,
  `suggest_share_name`, and `share_tree`'s `base_path`) take `&Path` /
  `Option<&Path>` instead, since they never open a file. Passphrases are an
  `impl Into<Passphrase>` over the borrowed shapes `&str`, `&String`, `&[u8]`,
  `&[u8; N]`, `&Vec<u8>`, `&Zeroizing<Vec<u8>>` and `&Zeroizing<String>`; the
  non-UTF-8 check happens once, in `Passphrase::as_utf8()`.
- The config read is bounded and binary. `read_file()` caps at one mebibyte;
  `config.pwd` is an rkyv *binary* document, not UTF-8 text, so no byte-order
  mark can arise on this path — the BOM strip in `import_tree()` applies to
  imports only.
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
  (`tempfile`'s `persist_noclobber`: `renameat_with(RENAME_NOREPLACE)` where the
  OS and filesystem support it, a `hard_link` + `unlink` fallback otherwise), so
  an existing file (or a symlink) is never overwritten and no hard kill can leave
  a zero-byte placeholder.
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
- The XML and CSV parsers are hand-rolled to avoid pulling in parser crates; each format lives in one module (`xml.rs`, `csv.rs`) that owns both its reader and its writer, so its escaping cannot drift between the two directions. Max XML nesting depth is 256 and max CSV path depth is also 256.
- `FileEntry` is the public type for directory listings; `file_browser` re-exports it.
- `DiskInfo` is the public type for mounted disks; `file_browser` consumes it via `list_disks()`.

For file_browser keybindings see [ADR 0007 — file_browser](../../file_browser/docs/adr/0007-file-browser.md).

## Testing the write path

`atomic.rs` is a thin wrapper over the write primitives (`TempFile` create /
`write` / `fsync` / `rename`, or the `link` + `unlink` fallback where a
no-clobber rename is unavailable) plus the symlink-refusing `open_for_read`.
It is tested against a real `tempfile::tempdir()` by design: the value of the
tests is that they exercise the kernel's actual behaviour (an exclusive random
temp name, a preserved mode, a symlink that is not followed, a no-clobber
rename), which a mock would not prove. **No filesystem seam is introduced.**

The consequence is deliberate and accepted: the failure branches that need a
hostile filesystem — `EACCES`, `EIO`, an `fsync` failure, and the mid-write
`WriteError::Io` path in the atomic write primitive (the shared code behind
`save_tree`, `create_secret_file`, `write_file` and the exports) — cannot be
injected and are therefore not unit-tested. The classified failures that *can*
be produced with a real filesystem are covered: an occupied target
(`AlreadyExists`, an existing file), a missing directory (`NotWritable`), and a
cancelled write (`Abandoned`). Where injection *is* needed, it already exists
one level up: `Autosave` takes its save as a `Fn` closure, so the worker's
scheduling, cancellation and zeroization can be driven with a stub write
without touching `atomic.rs`. If a future change makes a syscall failure branch
load-bearing, revisit this decision and add a narrow `trait WriteTarget` at
that point rather than pre-emptively.

## Dependencies

- `age` 0.11 — encryption
- `serde` 1 + `serde_json` 1 — tree serialization
- `sysinfo` 0.38 (`disk` feature only) — cross-platform mounted-disk enumeration
- `tempfile` 3 — the atomic layer's exclusively created temp file and its
  `persist` / `persist_noclobber` rename (`atomic.rs`)
- `std` only for autosave — `std::thread`, `Mutex`, `Condvar`; no async runtime in this module