# 0009 — cli module

CLI lifecycle subcommands that operate on the installation itself rather than
on secret data. Only `remove` remains: `update` (self-update over the network)
was removed so the binary makes no network requests (see
[ADR 0027](../../../../docs/adr/0027-no-in-app-network-access.md)).

**Decision:** keep the CLI command in a separate `cli` module with a
facade file (`src/cli.rs`) and **private** sub-modules under `src/cli/`.
The only public surface is the [`LifecycleCommand`](../../../cli.rs) enum
(`parse` + `run`) and the `EXIT_FOREIGN_INSTALL` constant, so `main.rs` is a
pure parser + dispatcher. The module has no TUI dependencies and no network
dependencies — it only prints to stdout/stderr and exits, and isolates
platform-specific logic (artefact layout mirroring, detached self-remove
staging on Windows).

**Rejected alternative:** inline remove logic in `main.rs`. Would bloat the
binary entry point with platform-conditional code and I/O orchestration.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `detect.rs` | `InstallSource` enum (SelfManaged/Pacman/CargoInstall/Unknown) with `detect()` / `detect_from(exe)` and `require_self_managed() -> Result<(), ForeignInstall>`; per-platform binary-path heuristics |
| `layout.rs` | **Single Rust owner of the install layout**: `home_dir`, `bin_dir`, `binary_path`, `install_dir`, `completions`, `rc_files`, `gui_artifacts`, `powershell_profiles`, `start_menu_shortcut`, plus the safe `TempDir` primitive |
| `remove.rs` | `run_remove()` — collects all installed artifacts (binary, config dir, completions, rc marker blocks, GUI artifacts, PowerShell profile, User `PATH`), prompts for confirmation, deletes/edits them; on Windows spawns a detached helper for self-remove of the locked binary |

## Contracts

- `remove` refuses to operate unless `InstallSource::require_self_managed()` passes; a foreign install returns `ExitCode::from(EXIT_FOREIGN_INSTALL)` (`2`).
- `require_self_managed` returns a typed `ForeignInstall(InstallSource)` error implementing `Display` + `Error`, so callers can distinguish pacman from cargo without parsing text.
- Neither the module nor the binary makes any network request; `update` was removed in [ADR 0027](../../../../docs/adr/0027-no-in-app-network-access.md).
- `remove` reads every path from `layout.rs`; it mirrors the installer layout in reverse.
- `remove` exit codes: `0` success (including a user abort), `2` foreign install, `1` when any artifact could not be removed.
- `remove` returns `ExitCode` directly (no `Result` propagation up).

## Dependencies

- `vault::utils` — `config_dir()` for determining the config directory to remove
- `uuid` — unpredictable temporary-directory suffixes (already a crate dependency)
- `std` otherwise — no new crates
