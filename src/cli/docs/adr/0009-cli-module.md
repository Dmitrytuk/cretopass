# 0009 — cli module

CLI lifecycle subcommands (`update`, `remove`) that operate on the
installation itself rather than on secret data.

**Decision:** keep the CLI commands in a separate `cli` module with a
facade file (`src/cli.rs`) and sub-modules under `src/cli/`. The module
has no TUI dependencies — it only prints to stdout/stderr and exits.
This keeps `main.rs` thin (argument parsing + dispatch) and isolates
platform-specific logic (Unix shell vs PowerShell, detached helpers,
self-remove staging).

**Rejected alternative:** inline update/remove logic in `main.rs`. Would
bloat the binary entry point with platform-conditional code, shell
scripting, and I/O orchestration.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `detect.rs` | `InstallSource` enum (SelfManaged/Pacman/CargoInstall/Unknown) with `detect()` and `require_self_managed()`; per-platform binary-path heuristics |
| `update.rs` | `run_update()` — downloads `install.sh`/`install.ps1` from GitHub Releases and runs it; on Windows spawns a detached helper for self-replace |
| `remove.rs` | `run_remove()` — collects all installed artifacts (binary, config dir, completions, rc PATH blocks, GUI artifacts), prompts for confirmation, deletes them; on Windows spawns a detached helper for self-remove of the locked binary |

## Contracts

- `update` and `remove` refuse to operate unless `InstallSource::require_self_managed()` passes (exit code 2 for foreign installs).
- `update` delegates entirely to the platform installer script — no version comparison in Rust.
- `remove` hardcodes the same paths the installer creates; it mirrors the installer layout in reverse.
- Both commands return `ExitCode` directly (no `Result` propagation up).

## Dependencies

- `vault::utils` — `config_dir()` for determining the config directory to remove
- `std` only — no external crates