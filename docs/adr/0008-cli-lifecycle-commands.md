# 0008 — CLI lifecycle commands (`update`, `remove`)

`cretopass update` delegates to the platform installer script;
`cretopass remove` mirrors the installer's layout in reverse.
On Windows, both use a detached helper to stage self-replace/self-remove.

## Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | **Only self-managed installs.** `update`/`remove` work exclusively with GitHub Releases + `install.sh`/`install.ps1`. AUR/pacman and `cargo install` are detected and refused with a hint to use the system package manager. | Prevents dangerous cross-tooling (pacman-owned files overwritten by direct download). |
| 2 | **`update` delegates to the installer.** Rust downloads `install.sh`/`install.ps1` from `releases/latest/download/` and runs it. No version comparison in the binary. | Single source of truth for version resolution, checksum verification, and artifact layout. Avoids Rust↔shell drift. |
| 3 | **Windows: detached helper for self-replace/self-remove.** On Windows the running `.exe` is locked. `update`/`remove` prompt the user to close other cretopass windows, do the prep work, spawn a detached helper process, and exit. The helper waits ~1–2 s then replaces/removes the binary. Linux/macOS use direct unlink/rename (kernel keeps the old inode alive). | Self-exclusion on Windows is the only platform where the running binary is truly locked. The unconditional prompt is simpler than process detection and avoids race conditions. |
| 4 | **PATH rc-edit is opt-in.** The installer warns when `$INSTALL_DIR` is missing from PATH and offers to append it (`[y/N]`). Non-interactive sessions skip the prompt silently. Marker-wrapped block: `# >>> cretopass PATH >>>` / `# <<< cretopass PATH <<<`. Fish uses `fish_add_path`. | Avoids silent corruption of user shell configs (trailing-newline gluing, path-shadowing). Explicit confirmation puts the user in control. |
| 5 | **Completions are installed (placeholder content for now).** `install.sh`/`install.ps1` fetch completions from `releases/latest/download/` and place them in shell-standard directories. `remove` cleans them up. | Infrastructure is ready now; actual content can be backfilled later (CI generation or hand-written). |

## Cross-cutting consequences

- `remove` must mirror the full installer layout: binary, completions, rc PATH block, GUI artifacts (`.desktop` + icon, `.app` bundle, Start Menu shortcut), and config directory.
- `remove` prompts for confirmation before deleting, listing what will be removed.
- `update` streams the installer's stdout/stderr to the user as a progress report.
- The `install.sh`/`install.ps1` scripts are the **canonical record** of installation layout. `remove` reads no schema — it hardcodes the same paths.