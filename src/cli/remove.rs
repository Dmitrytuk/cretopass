use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[cfg(any(target_os = "linux", windows))]
use std::process::{Command, Stdio};

use crate::vault::utils;

use super::detect::InstallSource;
use super::layout;

#[cfg(windows)]
use super::layout::TempDir;

/// Runs the `cretopass remove` subcommand.
///
/// Deletes all artifacts installed by `install.sh`/`install.ps1`: binary,
/// config directory, shell completions, GUI artifacts, and the rc PATH and
/// zsh-completion marker blocks.
///
/// Exit status: `0` on success and on a user abort; `2` for a foreign install
/// (see [`super::EXIT_FOREIGN_INSTALL`]); `1` when any artifact could not be
/// removed.
pub(crate) fn run_remove() -> ExitCode {
    if let Err(err) = InstallSource::detect().require_self_managed() {
        eprintln!("cretopass remove: {err}");
        return ExitCode::from(super::EXIT_FOREIGN_INSTALL);
    }

    let Some(home) = layout::home_dir() else {
        eprintln!("cretopass remove: cannot determine home directory");
        return ExitCode::FAILURE;
    };

    let artifacts = collect_artifacts(&home);
    if artifacts.is_empty() {
        println!("No cretopass artifacts found to remove.");
        return ExitCode::SUCCESS;
    }

    // Print what will be removed: deletions first, then the rc files that are
    // edited rather than deleted. `partition` returns the `true` group first.
    let (edits, deletions): (Vec<&Artifact>, Vec<&Artifact>) =
        artifacts.iter().partition(|art| art.is_edit());

    if !deletions.is_empty() {
        println!("The following will be deleted:");
        for art in &deletions {
            println!("  • {art}");
        }
    }
    if !edits.is_empty() {
        println!("\nThe following will be cleaned up (PATH block removed):");
        for art in &edits {
            println!("  • {art}");
        }
    }

    // Prompt for confirmation. Declining is not a failure.
    print!("\nRemove everything listed above? [y/N] ");
    if io::stdout().flush().is_err() {
        return ExitCode::FAILURE;
    }
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        eprintln!("cretopass remove: failed to read input");
        return ExitCode::FAILURE;
    }
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        println!("Aborted.");
        return ExitCode::SUCCESS;
    }

    #[cfg(windows)]
    {
        // Prompt to close cretopass before removing.
        println!("\ncretopass must not be running during removal.");
        println!(
            "Close any open cretopass windows, then press Enter to continue (Ctrl+C to cancel)."
        );
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            eprintln!("cretopass remove: failed to read input");
            return ExitCode::FAILURE;
        }
    }

    let failures = remove_artifacts(&artifacts, &home);

    #[cfg(windows)]
    let failures = {
        let mut failures = failures;
        // Spawn a detached helper to delete the locked binary + install dir
        // after the current process exits. If it cannot be staged, the binary
        // remains, so this is a failure.
        if !spawn_remove_helper_windows(&home) {
            eprintln!(
                "cretopass remove: failed to stage the self-remove helper; \
                 the cretopass binary may remain in place"
            );
            failures += 1;
        }
        failures
    };

    if failures == 0 {
        println!("cretopass has been removed.");
        ExitCode::SUCCESS
    } else {
        eprintln!("cretopass remove: {failures} artifact(s) could not be removed.");
        ExitCode::FAILURE
    }
}

/// A removable artifact.
///
/// Modelling the operation in the type removes the need to re-derive
/// "edit, don't delete" from a filename.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Artifact {
    /// A file that is deleted.
    DeleteFile(PathBuf),
    /// A directory tree that is deleted recursively.
    DeleteDir(PathBuf),
    /// A shell rc file from which the marker block is stripped.
    #[cfg(unix)]
    EditRc(PathBuf),
}

impl Artifact {
    /// Returns the artifact's path.
    fn path(&self) -> &Path {
        match self {
            Self::DeleteFile(path) | Self::DeleteDir(path) => path,
            #[cfg(unix)]
            Self::EditRc(path) => path,
        }
    }

    /// Returns true for a file that is edited rather than deleted.
    fn is_edit(&self) -> bool {
        #[cfg(unix)]
        {
            matches!(self, Self::EditRc(_))
        }
        #[cfg(not(unix))]
        {
            false
        }
    }
}

impl fmt::Display for Artifact {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.path().display())
    }
}

/// Collects every artifact the installer may have created.
fn collect_artifacts(home: &Path) -> Vec<Artifact> {
    let mut out = Vec::new();

    // --- Binary ---
    let bin_path = layout::binary_path(home);
    if bin_path.exists() {
        out.push(Artifact::DeleteFile(bin_path));
    }

    // --- Config directory ---
    let config_dir = utils::config_dir();
    if config_dir.exists() {
        if safe_to_delete(&config_dir, home) {
            out.push(Artifact::DeleteDir(config_dir));
        } else {
            eprintln!(
                "cretopass remove: refusing to delete suspicious config path {}",
                config_dir.display()
            );
        }
    }

    // --- Completions ---
    for path in layout::completions(home) {
        if path.exists() {
            out.push(Artifact::DeleteFile(path));
        }
    }

    // --- Shell rc marker blocks (Unix) ---
    #[cfg(unix)]
    collect_rc_path_blocks(home, &mut out);

    // --- GUI payload ---
    for path in layout::gui_artifacts(home) {
        if path.exists() {
            if path.is_dir() {
                out.push(Artifact::DeleteDir(path));
            } else {
                out.push(Artifact::DeleteFile(path));
            }
        }
    }

    // --- Start Menu shortcut (Windows) ---
    #[cfg(windows)]
    {
        if let Some(link) = layout::start_menu_shortcut()
            && link.exists()
        {
            out.push(Artifact::DeleteFile(link));
        }
    }

    out
}

/// Deletes every artifact and returns the number of failures.
fn remove_artifacts(artifacts: &[Artifact], home: &Path) -> usize {
    let mut failures = 0;

    for art in artifacts {
        // On Windows the running binary and its directory are locked; the
        // detached helper deletes them after this process exits.
        #[cfg(windows)]
        {
            if art.path().starts_with(layout::install_dir(home)) {
                continue;
            }
        }

        match art {
            Artifact::DeleteFile(path) => {
                if let Err(e) = std::fs::remove_file(path)
                    && path.exists()
                {
                    eprintln!("cretopass remove: failed to remove {}: {e}", path.display());
                    failures += 1;
                }
            }
            Artifact::DeleteDir(path) => {
                if let Err(e) = std::fs::remove_dir_all(path)
                    && path.exists()
                {
                    eprintln!("cretopass remove: failed to remove {}: {e}", path.display());
                    failures += 1;
                }
            }
            #[cfg(unix)]
            Artifact::EditRc(path) => {
                failures += edit_rc_file(path);
            }
        }
    }

    #[cfg(windows)]
    {
        failures += remove_pwsh_profile_lines(home);
        failures += remove_user_path_entry(home);
    }

    // Refresh the Linux desktop/icon caches only after a confirmed removal,
    // and only when the relevant artifacts existed.
    #[cfg(target_os = "linux")]
    {
        let gui = layout::gui_artifacts(home);
        if artifacts
            .iter()
            .any(|art| gui.iter().any(|path| path == art.path()))
        {
            refresh_desktop_caches(home);
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = home;

    failures
}

/// Refuses deletion targets that are obviously wrong: a relative path, the
/// filesystem root, the user's home directory, or anything not actually named
/// `cretopass`.
fn safe_to_delete(path: &Path, home: &Path) -> bool {
    if path.as_os_str().is_empty() || !path.is_absolute() {
        return false;
    }
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if canonical.parent().is_none() {
        return false;
    }
    let home_canonical = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    canonical != home_canonical
        && canonical.file_name().and_then(|name| name.to_str()) == Some("cretopass")
}

// ---------------------------------------------------------------------------
// Shell rc marker blocks (unix)
// ---------------------------------------------------------------------------

#[cfg(unix)]
const PATH_MARKER_START: &str = "# >>> cretopass PATH >>>";
#[cfg(unix)]
const PATH_MARKER_END: &str = "# <<< cretopass PATH <<<";
#[cfg(unix)]
const ZSH_COMPLETION_MARKER_START: &str = "# >>> cretopass completion >>>";
#[cfg(unix)]
const ZSH_COMPLETION_MARKER_END: &str = "# <<< cretopass completion <<<";

/// Adds the rc files that actually contain a cretopass marker block.
///
/// A file without any marker is not listed, because nothing about it would
/// change. A file that cannot be read is still listed so that `edit_rc_file`
/// reports the failure (rather than `remove` silently exiting `0` with markers
/// left behind).
#[cfg(unix)]
fn collect_rc_path_blocks(home: &Path, out: &mut Vec<Artifact>) {
    for path in layout::rc_files(home) {
        if !path.exists() {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(content)
                if content.contains(PATH_MARKER_START)
                    || content.contains(ZSH_COMPLETION_MARKER_START) =>
            {
                out.push(Artifact::EditRc(path));
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("cretopass remove: failed to read {}: {e}", path.display());
                out.push(Artifact::EditRc(path));
            }
        }
    }
}

/// Strips both cretopass marker blocks from one rc file, atomically.
///
/// Returns `1` when the file could not be read or rewritten.
#[cfg(unix)]
fn edit_rc_file(path: &Path) -> usize {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("cretopass remove: failed to read {}: {e}", path.display());
            return 1;
        }
    };

    let cleaned = remove_marker_block(&content, PATH_MARKER_START, PATH_MARKER_END);
    let cleaned = remove_marker_block(
        &cleaned,
        ZSH_COMPLETION_MARKER_START,
        ZSH_COMPLETION_MARKER_END,
    );

    if cleaned == content {
        if content.contains(PATH_MARKER_START) || content.contains(ZSH_COMPLETION_MARKER_START) {
            eprintln!(
                "cretopass remove: {} has an unterminated cretopass marker block; left unchanged",
                path.display()
            );
        }
        return 0;
    }
    if let Err(e) = write_atomic(path, &cleaned) {
        eprintln!("cretopass remove: failed to update {}: {e}", path.display());
        return 1;
    }
    0
}

/// Removes a marker-wrapped block (inclusive) from text content.
///
/// Removes all lines from (and including) `start_marker` through (and
/// including) `end_marker`, preserving every other line byte-for-byte
/// (including line endings and any trailing newline).
///
/// If a start marker has no matching whole-line end marker — which happens
/// after an interrupted install — the content is returned **unchanged** so the
/// rest of the user's file is never truncated. Markers are matched by whole
/// line, so an annotated/edited marker line never counts.
#[cfg(unix)]
fn remove_marker_block(content: &str, start_marker: &str, end_marker: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut skipping = false;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim();
        if !skipping && trimmed == start_marker {
            skipping = true;
            continue;
        }
        if skipping {
            if trimmed == end_marker {
                skipping = false;
            }
            continue;
        }
        result.push_str(line);
    }

    if skipping {
        // The start marker was never closed; leave the file untouched.
        return content.to_string();
    }
    result
}

/// Writes `content` to `path` via a same-directory temp file + rename, so an
/// interrupted write cannot truncate the user's file.
///
/// The temp file is created with [`layout::create_new_file`] under a random
/// name so it cannot be pre-created or symlinked by an attacker, and the
/// original file's permissions are preserved across the rename.
fn write_atomic(path: &Path, content: &str) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "cretopass".to_string());
    let tmp = parent.join(format!(
        ".{file_name}.{}.cretopass-tmp",
        uuid::Uuid::new_v4()
    ));

    let write_result = (|| -> io::Result<()> {
        let mut file = layout::create_new_file(&tmp)?;
        file.write_all(content.as_bytes())
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    #[cfg(unix)]
    if let Ok(metadata) = std::fs::metadata(path) {
        // Best-effort: preserve the original mode. If this fails the rewritten
        // file keeps the default mode rather than aborting the cleanup.
        let _ = std::fs::set_permissions(&tmp, metadata.permissions());
    }

    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

// ---------------------------------------------------------------------------
// PowerShell profile (Windows)
// ---------------------------------------------------------------------------

/// Removes the cretopass dot-source line from every candidate profile.
///
/// Returns the number of files that could not be rewritten.
#[cfg(windows)]
fn remove_pwsh_profile_lines(home: &Path) -> usize {
    let mut failures = 0;
    for profile in layout::powershell_profiles(home) {
        if !profile.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&profile) {
            Ok(content) => content,
            Err(e) => {
                eprintln!(
                    "cretopass remove: failed to read PowerShell profile {}: {e}",
                    profile.display()
                );
                failures += 1;
                continue;
            }
        };
        let filtered = strip_lines_containing(&content, "cretopass.ps1");
        if filtered != content
            && let Err(e) = write_atomic(&profile, &filtered)
        {
            eprintln!(
                "cretopass remove: failed to update PowerShell profile {}: {e}",
                profile.display()
            );
            failures += 1;
        }
    }
    failures
}

/// Drops every line containing `needle`, preserving all other bytes.
#[cfg(windows)]
fn strip_lines_containing(content: &str, needle: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for line in content.split_inclusive('\n') {
        if !line.contains(needle) {
            out.push_str(line);
        }
    }
    out
}

/// Removes the install directory from the registry User `PATH`.
///
/// Returns `1` when PowerShell failed to rewrite the value. The install
/// directory is passed through the environment and read as data, so it can
/// never be interpreted as PowerShell code.
#[cfg(windows)]
fn remove_user_path_entry(home: &Path) -> usize {
    const SCRIPT: &str = r"
$target = $env:CRETOPASS_PATH_ENTRY
$current = [Environment]::GetEnvironmentVariable('Path','User')
if (-not $current) { exit 0 }
$kept = $current -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ne $target.TrimEnd('\') }
[Environment]::SetEnvironmentVariable('Path', ($kept -join ';'), 'User')
";

    let status = Command::new("powershell")
        .env("CRETOPASS_PATH_ENTRY", layout::install_dir(home))
        .args(["-NoProfile", "-Command", SCRIPT])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();

    match status {
        Ok(status) if status.success() => 0,
        _ => 1,
    }
}

// ---------------------------------------------------------------------------
// GUI caches (Linux)
// ---------------------------------------------------------------------------

/// Refreshes the desktop and icon caches after removal (best-effort).
#[cfg(target_os = "linux")]
fn refresh_desktop_caches(home: &Path) {
    // Best-effort: a missing cache tool or a non-zero status is not a failure.
    let _ = Command::new("update-desktop-database")
        .arg(layout::desktop_dir(home))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = Command::new("gtk-update-icon-cache")
        .args(["-f", "-t"])
        .arg(layout::icon_theme_dir(home))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

// ---------------------------------------------------------------------------
// Windows detached helper for self-remove
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn spawn_remove_helper_windows(home: &Path) -> bool {
    let install_dir = layout::install_dir(home);
    let tmp = match TempDir::create("cretopass-remove") {
        Ok(dir) => dir,
        Err(_) => return false,
    };
    let helper_path = tmp.path().join("_remove_helper.bat");

    for path in [&install_dir, &helper_path, tmp.path()] {
        if layout::ensure_safe_windows_path(path).is_err() {
            return false;
        }
    }

    let helper_content = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak > nul\r\n\
         if exist \"{install}\\cretopass.exe\" del /f /q \"{install}\\cretopass.exe\"\r\n\
         if exist \"{install}\" rmdir /q /s \"{install}\" 2>nul\r\n\
         del /f /q \"{helper}\"\r\n\
         rmdir /q /s \"{tmp}\" 2>nul\r\n",
        install = install_dir.display(),
        helper = helper_path.display(),
        tmp = tmp.path().display(),
    );

    if std::fs::write(&helper_path, helper_content).is_err() {
        return false;
    }

    let spawned = Command::new("cmd")
        .args(["/c", "start", "", "/b"])
        .arg(&helper_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match spawned {
        Ok(_) => {
            // The helper reads and removes the script after we exit.
            tmp.keep();
            true
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asserts that a marker-wrapped block is removed while surrounding
    /// content — including its trailing newline — is preserved.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_strips_block() {
        let content = "line before\n# >>> cretopass PATH >>>\nexport PATH=...\n# <<< cretopass PATH <<<\nline after\n";
        let cleaned = remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END);
        assert_eq!(cleaned, "line before\nline after\n");
    }

    /// Returns content unchanged when neither marker is present.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_missing_keeps_content() {
        let content = "no markers here";
        assert_eq!(
            remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END),
            content
        );
    }

    /// A dangling start marker (no end marker) must not truncate the file.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_unterminated_start_keeps_content() {
        let content = "keep\n# >>> cretopass PATH >>>\ntrailing\n";
        assert_eq!(
            remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END),
            content
        );
    }

    /// An empty block (markers on adjacent lines) is fully removed.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_empty_block() {
        let content = "keep\n# >>> cretopass PATH >>>\n# <<< cretopass PATH <<<\nkeep\n";
        let cleaned = remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END);
        assert_eq!(cleaned, "keep\nkeep\n");
    }

    /// CRLF line endings survive the rewrite.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_preserves_crlf() {
        let content = "keep\r\n# >>> cretopass PATH >>>\r\nexport PATH=...\r\n# <<< cretopass PATH <<<\r\nkeep\r\n";
        let cleaned = remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END);
        assert_eq!(cleaned, "keep\r\nkeep\r\n");
    }

    /// Two different marker blocks (PATH and zsh completion) are both removed.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_multiple_blocks() {
        let content = "# >>> cretopass PATH >>>\nexport PATH=...\n# <<< cretopass PATH <<<\n\
# >>> cretopass completion >>>\nfpath=(...)\n# <<< cretopass completion <<<\n";
        let cleaned = remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END);
        let cleaned = remove_marker_block(
            &cleaned,
            ZSH_COMPLETION_MARKER_START,
            ZSH_COMPLETION_MARKER_END,
        );
        assert_eq!(cleaned, "");
    }

    /// An interrupted install that appended a second dangling start marker
    /// after a complete block must leave the whole file untouched.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_dangling_start_after_block_keeps_content() {
        let content = "export PATH=$PATH\n\
# >>> cretopass PATH >>>\n# <<< cretopass PATH <<<\n\
# >>> cretopass PATH >>>\necho done\nalias ll=ls\n";
        assert_eq!(
            remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END),
            content
        );
    }

    /// An annotated end-marker line does not close the block, so nothing is
    /// removed and no content is lost.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_annotated_end_keeps_content() {
        let content = "keep\n# >>> cretopass PATH >>>\nexport PATH=X\n\
# <<< cretopass PATH <<< oops\nalias foo=bar\n";
        assert_eq!(
            remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END),
            content
        );
    }

    /// An orphan end marker before a real block is kept; the real block is
    /// still removed.
    #[test]
    #[cfg(unix)]
    fn remove_marker_block_orphan_end_before_block() {
        let content = "# <<< cretopass PATH <<<\nbefore\n# >>> cretopass PATH >>>\n\
export PATH=X\n# <<< cretopass PATH <<<\nafter\n";
        let cleaned = remove_marker_block(content, PATH_MARKER_START, PATH_MARKER_END);
        assert_eq!(cleaned, "# <<< cretopass PATH <<<\nbefore\nafter\n");
    }

    /// A relative config path is refused so `remove` cannot act on the cwd.
    #[test]
    fn safe_to_delete_rejects_relative_paths() {
        let home = std::env::temp_dir();
        assert!(!safe_to_delete(Path::new(".cretopass"), &home));
    }

    /// The user's home directory itself is never a deletion target.
    #[test]
    fn safe_to_delete_rejects_home() {
        let home = std::env::temp_dir();
        assert!(!safe_to_delete(&home, &home));
    }

    /// The filesystem root is refused.
    #[test]
    fn safe_to_delete_rejects_root() {
        assert!(!safe_to_delete(Path::new("/"), Path::new("/home/user")));
    }

    /// A real nested directory is accepted.
    #[test]
    fn safe_to_delete_accepts_nested_dir() {
        let home = std::env::temp_dir().join(format!("cretopass-safe-{}", uuid::Uuid::new_v4()));
        let config = home.join(".config/cretopass");
        std::fs::create_dir_all(&config).unwrap();
        assert!(safe_to_delete(&config, &home));
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A failed deletion is counted so `run_remove` can report a non-zero exit.
    #[test]
    #[cfg(unix)]
    fn remove_artifacts_counts_failures() {
        use std::os::unix::fs::PermissionsExt;

        let root = std::env::temp_dir().join(format!("cretopass-fail-{}", uuid::Uuid::new_v4()));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("file"), b"x").unwrap();

        // A read-only parent prevents removing its non-empty child.
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o500)).unwrap();

        let artifacts = vec![Artifact::DeleteDir(nested.clone())];
        let failures = remove_artifacts(&artifacts, &root);

        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let remained = nested.exists();
        let _ = std::fs::remove_dir_all(&root);

        if remained {
            assert_eq!(failures, 1);
        } else {
            // Running as root bypasses directory permissions; nothing to assert.
            eprintln!("skipping failure assertion: deletion could not be blocked");
        }
    }
}
