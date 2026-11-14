# 0009 — cli module

CLI entry point and lifecycle subcommands that operate on the installation
itself rather than on secret data. Only `remove` remains: `update`
(self-update over the network) was removed so the binary makes no network
requests (see [ADR 0027](../../../../docs/adr/0027-no-in-app-network-access.md)).

**Decision:** keep the CLI entry in a separate `cli` module with a facade file
(`src/cli.rs`) and **private** sub-modules under `src/cli/`. `cli::run()` is the
binary's entry point: it parses `argv`, dispatches the one lifecycle subcommand
(`remove`), and — when no subcommand is given — starts the TUI through
`app::run()`. `main.rs` therefore stays a three-line shell. The lifecycle
sub-modules (`detect`, `layout`, `remove`) have **no TUI dependencies** and no
network dependencies: they only print to stdout/stderr and exit, and isolate
platform-specific logic (artefact layout mirroring, detached self-remove staging
on Windows). Only the TUI dispatch arm reaches the rest of the app (`app` →
`vault` → `tree`).

**Rejected alternative:** inline remove logic in `main.rs`. Would bloat the
binary entry point with platform-conditional code and I/O orchestration.

## Sub-modules

| File | Responsibility |
|------|---------------|
| `detect.rs` | `InstallSource` enum (SelfManaged/Pacman/CargoInstall/Unknown) with `detect()` / `detect_from(exe)` and `require_self_managed() -> Result<(), ForeignInstall>`; per-platform binary-path heuristics |
| `layout.rs` | **Single Rust owner of the install layout**: `home_dir`, `bin_dir`, `binary_path`, `install_dir`, `completions`, `rc_files`, `gui_artifacts`, `powershell_profiles`, `start_menu_shortcut`, plus the safe `TempDir` primitive |
| `remove.rs` | `run_remove()` — collects all installed artifacts (binary, config dir, completions, rc marker blocks, GUI artifacts, PowerShell profile, User `PATH`), prompts for confirmation, deletes/edits them; on Windows spawns a detached helper for self-remove of the locked binary |

## Contracts

- `cli::run()` is the only entry point: it parses `argv` and either dispatches
  a lifecycle subcommand or hands control to `app::run()`. [`LifecycleCommand`](../../../cli.rs)
  (`parse` + `run`) and `EXIT_FOREIGN_INSTALL` are its dispatch vocabulary, not a
  public API — the crate has no library target.
- `remove` refuses to operate unless `InstallSource::require_self_managed()` passes; a foreign install returns `ExitCode::from(EXIT_FOREIGN_INSTALL)` (`2`).
- `require_self_managed` returns a typed `ForeignInstall(InstallSource)` error implementing `Display` + `Error`, so callers can distinguish pacman from cargo without parsing text.
- Neither the module nor the binary makes any network request; `update` was removed in [ADR 0027](../../../../docs/adr/0027-no-in-app-network-access.md).
- `remove` reads every path from `layout.rs`; it mirrors the installer layout in reverse.
- `remove` exit codes: `0` success (including a user abort), `2` foreign install, `1` when any artifact could not be removed.
- `remove` returns `ExitCode` directly (no `Result` propagation up).

## Dependencies

- `app` — starts the TUI when no lifecycle subcommand is given
- `theming::texts` — app identity for `--version` and the `--help` footer
- `vault::utils` — `config_dir()` for determining the config directory to remove
- `uuid` — unpredictable temporary-directory suffixes (already a crate dependency)
- `std` otherwise — no new crates
