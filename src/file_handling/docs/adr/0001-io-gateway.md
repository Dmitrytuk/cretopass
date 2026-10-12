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
| `file_process.rs` | JSON serde, raw read/write, encrypted file ops, directory listing, save with atomic temp-file+rename, `.bak` backup, path probes, age-header detection |
| `error.rs` | `FileHandlingError` enum wrapping Io / Serde / Crypt errors + domain errors |
| `export.rs` | `ExportFormat` (JSON/XML/CSV), recursive XML serialization, flattened CSV export |
| `import.rs` | `ImportFormat` detection, JSON/XML/CSV import with custom parsers, `build_tree_from_paths()` |

## Contracts

- All public functions accept `&str` or `&Path` for paths (not `PathBuf`).
- `save_tree()` is crash-safe: writes to `{path}.tmp` → `fsync` → copies original to `{path}.bak` → `rename(tmp, original)` → removes `.bak`.
- `validate_encrypted_file()` enforces a 50MB size cap and scans for the `age-encryption.org` magic header.
- Import parsers (XML, CSV) are hand-rolled to avoid pulling in parser crates. Max XML nesting depth is 256.
- `FileEntry` is the public type for directory listings; `file_browser` re-exports it.

For file_browser keybindings see [ADR 0007 — file_browser](../../file_browser/docs/adr/0007-file-browser.md).

## Dependencies

- `age` 0.11 — encryption
- `serde` 1 + `serde_json` 1 — tree serialization