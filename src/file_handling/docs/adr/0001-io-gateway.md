# 0001 — I/O gateway (`file_handling`)

The `file_handling` module is the **single point of filesystem access** for the
entire application. No other module imports `std::fs`. This was a deliberate
boundary decision: all disk I/O is mediated through this module so that
encryption, backup, error handling, and import/export logic share one place.

**Rejected alternative:** sprinkling `std::fs` calls across modules. This would
make it impossible to guarantee atomic saves, enforce encryption on write, or
audit I/O patterns.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `file_process.rs` | JSON serde, raw read/write, encrypted file ops, directory listing (visible or including hidden), save with atomic temp-file+rename, `.bak` backup, path probes, age-header detection |
| `disks.rs` | `DiskInfo` + `list_disks()`: cross-platform mounted-volume enumeration via `sysinfo`, with loop/squashfs filtering |
| `error.rs` | `FileHandlingError` enum wrapping Io / Serde / Crypt errors + domain errors |
| `export.rs` | `ExportFormat` (JSON/XML/CSV), recursive XML serialization, flattened CSV export |
| `import.rs` | `ImportFormat` detection, JSON/XML/CSV import with custom parsers, `build_tree_from_paths()` |

## Contracts

- All public functions accept `&str` or `&Path` for paths (not `PathBuf`).
- `save_tree()` is crash-safe: writes to `{path}.tmp` → `fsync` → copies original to `{path}.bak` → `rename(tmp, original)` → removes `.bak`.
- `validate_encrypted_file()` enforces a 50MB size cap and scans for the `age-encryption.org` magic header.
- `list_dir()` skips hidden (dot-prefixed) entries; `list_dir_including_hidden()` returns them too. Both sort directories first, then alphabetically.
- `list_disks()` returns project-owned `DiskInfo` rows sorted by mount point. `sysinfo` excludes pseudo filesystems, tmpfs, optical media, network shares, and loop-backed `squashfs` mounts; a defensive filter also drops `/dev/loop*` and snap mounts. Returns an empty vector when no disks are reported.
- Import parsers (XML, CSV) are hand-rolled to avoid pulling in parser crates. Max XML nesting depth is 256.
- `FileEntry` is the public type for directory listings; `file_browser` re-exports it.
- `DiskInfo` is the public type for mounted disks; `file_browser` consumes it via `list_disks()`.

For file_browser keybindings see [ADR 0007 — file_browser](../../file_browser/docs/adr/0007-file-browser.md).

## Dependencies

- `age` 0.11 — encryption
- `serde` 1 + `serde_json` 1 — tree serialization
- `sysinfo` 0.38 (`disk` feature only) — cross-platform mounted-disk enumeration