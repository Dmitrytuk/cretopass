# 0008 — CLI lifecycle commands (`update`, `remove`)

`cretopass update` delegates to the platform installer script;
`cretopass remove` mirrors the installer's layout in reverse.
On Windows, both use a detached helper to stage self-replace/self-remove.

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Only self-managed installs.** `update`/`remove` work exclusively with GitHub Releases + `install.sh`/`install.ps1`. AUR/pacman and `cargo install` are detected and refused with a hint to use the system package manager. | Prevents dangerous cross-tooling (pacman-owned files overwritten by direct download). AUR packaging (`PKGBUILD`, the man page, and shell completions) and its Docker test harness live in the dormant `archive/aur-packaging` branch, not on `develop`; the pacman detection above stays in the binary regardless. |
| 2 | **`update` delegates to the installer.** Rust downloads `install.sh`/`install.ps1` from `releases/latest/download/` and runs it. No version comparison in the binary. | Single source of truth for version resolution, checksum verification, and artifact layout. Avoids Rust↔shell drift. |
| 3 | **Windows: detached helper for self-replace/self-remove.** On Windows the running `.exe` is locked. `update`/`remove` prompt the user to close other cretopass windows, do the prep work, spawn a detached helper process, and exit. The helper waits ~1–2 s then replaces/removes the binary. Linux/macOS use direct unlink/rename (kernel keeps the old inode alive). | Self-exclusion on Windows is the only platform where the running binary is truly locked. The unconditional prompt is simpler than process detection and avoids race conditions. |
| 4 | **PATH rc-edit is opt-in.** The installer warns when `$INSTALL_DIR` is missing from PATH and offers to append it (`[y/N]`). Non-interactive sessions skip the prompt silently. Marker-wrapped block: `# >>> cretopass PATH >>>` / `# <<< cretopass PATH <<<`. Fish uses `fish_add_path`. | Avoids silent corruption of user shell configs (trailing-newline gluing, path-shadowing). Explicit confirmation puts the user in control. |
| 5 | **Completions are installed (placeholder content for now).** `install.sh`/`install.ps1` fetch completions from `releases/latest/download/` and place them in shell-standard directories. `remove` cleans them up. | Infrastructure is ready now; actual content can be backfilled later (CI generation or hand-written). |
| 6 | **One Rust layout owner.** `src/cli/layout.rs` holds every install path (`bin_dir`, `binary_path`, `install_dir`, `completions`, `rc_files`, `gui_artifacts`, `powershell_profiles`, Start Menu, temp dirs). `detect` and `remove` contain no hardcoded layout strings. | The layout previously lived in four unlinked copies (`detect` heuristics, `remove`'s artifact list, `install.sh`/`install.ps1`, test fixtures). That drift already shipped a Windows bug: `remove` edited `Microsoft.PowerShell_profile.ps1` while `install.ps1` wrote `$PROFILE.CurrentUserAllHosts`. One owner makes drift a compile-time concern. |
| 7 | **`update` pins the transport, not the payload.** The `curl` download is pinned to HTTPS (`--proto '=https' --proto-redir '=https'`) and the URL is passed after `--`, so it can never be parsed as an option. Payload checksum/signature verification is delegated to the installer and is **not yet** implemented in Rust. | The binary has no copy of the release checksums, and the installer already verifies them. Adding real verification (published `.sha256`/signature) is tracked as a follow-up; pinning the transport closes the TLS-downgrade/argument-injection gap now. |
| 8 | **Exit-code contract.** `0` = success (including a user-aborted `remove`), `2` = foreign install (`EXIT_FOREIGN_INSTALL`), `1` = failure (a staged `remove` left artifacts behind). On Windows, `update` reports staging success rather than the installer's eventual result. | A user needs to distinguish "the system is clean" from "something refused to delete". The abort path is documented as success because nothing was attempted. |

## Cross-cutting consequences

- `remove` mirrors the full installer layout via [`layout`](../../src/cli/layout.rs): binary, completions, rc PATH block, GUI artifacts (`.desktop` + icon, `.app` bundle, Start Menu shortcut on both PowerShell profile candidates), User `PATH` and the config directory.
- `remove` prompts for confirmation before deleting, listing what will be removed, and lists rc files only when they actually contain a cretopass marker.
- `remove` rewrites shell rc files atomically and never truncates them when a marker block is unterminated.
- `update` streams the installer's stdout/stderr to the user as a progress report.
- The `install.sh`/`install.ps1` scripts are the **canonical record** of installation layout; `layout.rs` is the Rust mirror and the only place `detect`/`remove` read it from.
- `CRETOPASS_UPDATE_URL` is honoured in debug builds only; `CRETOPASS_CONFIG_DIR` is likewise debug-gated in [`vault::utils::config_dir`](../../src/vault/utils.rs).
