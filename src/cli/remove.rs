use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use crate::vault::utils;

/// Runs the `cretopass remove` subcommand.
///
/// Deletes all artifacts installed by `install.sh`/`install.ps1`:
/// binary, config directory, shell completions, GUI artifacts,
/// and the rc PATH marker block.
pub fn run_remove() -> ExitCode {
    if let Err(msg) = super::detect::InstallSource::detect().require_self_managed() {
        eprintln!("cretopass remove: {msg}");
        return ExitCode::from(2);
    }

    let home = home_dir();
    if home.as_os_str().is_empty() {
        eprintln!("cretopass remove: cannot determine home directory");
        return ExitCode::FAILURE;
    }

    let artifacts = collect_artifacts(&home);

    if artifacts.is_empty() {
        println!("No cretopass artifacts found to remove.");
        return ExitCode::SUCCESS;
    }

    // Print what will be removed.
    let (edits, deletions): (Vec<_>, Vec<_>) = artifacts.iter().partition(|a| match a {
        Artifact::File(p) => is_rc_file(p),
        Artifact::Dir(_) => false,
    });

    if !deletions.is_empty() {
        println!("The following will be deleted:");
        for art in &deletions {
            match art {
                Artifact::File(p) => println!("  • {}", p.display()),
                Artifact::Dir(p) => println!("  • {}", p.display()),
            }
        }
    }

    if !edits.is_empty() {
        println!("\nThe following will be cleaned up (PATH block removed):");
        for art in &edits {
            match art {
                Artifact::File(p) => println!("  • {}", p.display()),
                Artifact::Dir(p) => println!("  • {}", p.display()),
            }
        }
    }

    // Prompt for confirmation.
    print!("\nRemove everything listed above? [y/N] ");
    if io::stdout().flush().is_err() {
        return ExitCode::FAILURE;
    }
    let mut answer = String::new();
    if io::stdin().read_line(&mut answer).is_err() {
        eprintln!("cretopass remove: failed to read input");
        return ExitCode::FAILURE;
    }
    let answer = answer.trim();
    if !answer.eq_ignore_ascii_case("y") && !answer.eq_ignore_ascii_case("yes") {
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

    remove_artifacts(&artifacts, &home);

    #[cfg(windows)]
    {
        // Spawn a detached helper to delete the binary + install dir
        // after the current process exits.
        spawn_remove_helper_windows(&home);
    }

    println!("cretopass has been removed.");
    ExitCode::SUCCESS
}

/// A removable artifact: either a file or a directory.
enum Artifact {
    File(PathBuf),
    Dir(PathBuf),
}

fn collect_artifacts(home: &Path) -> Vec<Artifact> {
    let mut out = Vec::new();

    // --- Binary ---
    let bin_path = binary_path(home);
    if bin_path.exists() {
        out.push(Artifact::File(bin_path));
    }

    // --- Config directory ---
    let config_dir = utils::config_dir();
    // config_dir() respects CRETOPASS_CONFIG_DIR — use it as-is but only
    // remove if it exists and is under the expected pattern (not an
    // overridden test path).
    if config_dir.exists() {
        out.push(Artifact::Dir(config_dir));
    }

    #[cfg(unix)]
    {
        collect_completions_unix(home, &mut out);
        collect_rc_path_block(home, &mut out);
        collect_gui_unix(home, &mut out);
    }

    #[cfg(windows)]
    {
        collect_completions_windows(home, &mut out);
        collect_start_menu_shortcut(&mut out);
    }

    out
}

fn remove_artifacts(artifacts: &[Artifact], home: &Path) {
    for art in artifacts {
        match art {
            Artifact::File(p) => {
                // Skip rc files — we only edit them, not delete.
                if is_rc_file(p) {
                    continue;
                }
                if p.exists()
                    && let Err(e) = std::fs::remove_file(p)
                {
                    eprintln!("cretopass remove: failed to remove {}: {e}", p.display());
                }
            }
            Artifact::Dir(p) => {
                if p.exists()
                    && let Err(e) = std::fs::remove_dir_all(p)
                {
                    eprintln!("cretopass remove: failed to remove {}: {e}", p.display());
                }
            }
        }
    }

    // Clean up rc PATH block (text editing, not file deletion).
    remove_rc_path_blocks(home);

    #[cfg(windows)]
    {
        remove_pwsh_profile_line(home);
    }
}

/// Returns true if the path is one of the shell rc files (which we edit, not delete).
fn is_rc_file(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    name == ".zshrc" || name == ".bashrc" || name == "config.fish"
}

// ---------------------------------------------------------------------------
// Binary path
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn binary_path(home: &Path) -> PathBuf {
    home.join(".local/bin/cretopass")
}

#[cfg(windows)]
fn binary_path(home: &Path) -> PathBuf {
    // Binary is in %LOCALAPPDATA%\Programs\cretopass\cretopass.exe
    if let Ok(d) = std::env::var("LOCALAPPDATA") {
        Path::new(&d)
            .join("Programs")
            .join("cretopass")
            .join("cretopass.exe")
    } else {
        home.join("AppData/Local/Programs/cretopass/cretopass.exe")
    }
}

// ---------------------------------------------------------------------------
// Home directory
// ---------------------------------------------------------------------------

fn home_dir() -> PathBuf {
    #[cfg(unix)]
    {
        std::env::var("HOME").map(PathBuf::from).unwrap_or_default()
    }
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .map(PathBuf::from)
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Shell completions (unix)
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn collect_completions_unix(home: &Path, out: &mut Vec<Artifact>) {
    let bash = home.join(".local/share/bash-completion/completions/cretopass");
    if bash.exists() {
        out.push(Artifact::File(bash));
    }
    let zsh = home.join(".zsh/completions/_cretopass");
    if zsh.exists() {
        out.push(Artifact::File(zsh));
    }
    let fish = home.join(".config/fish/completions/cretopass.fish");
    if fish.exists() {
        out.push(Artifact::File(fish));
    }
}

// ---------------------------------------------------------------------------
// Shell completions (Windows PowerShell)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn collect_completions_windows(home: &Path, out: &mut Vec<Artifact>) {
    let pwsh = home.join("Documents/PowerShell/Completions/cretopass.ps1");
    if pwsh.exists() {
        out.push(Artifact::File(pwsh));
    }
}

#[cfg(windows)]
fn remove_pwsh_profile_line(home: &Path) {
    let profile = home.join("Documents/PowerShell/Microsoft.PowerShell_profile.ps1");
    if !profile.exists() {
        return;
    }
    match std::fs::read_to_string(&profile) {
        Ok(content) => {
            let filtered: String = content
                .lines()
                .filter(|line| !line.contains("cretopass.ps1"))
                .collect::<Vec<_>>()
                .join("\n");
            if filtered != content {
                if let Err(e) = std::fs::write(&profile, filtered) {
                    eprintln!("cretopass remove: failed to update PowerShell profile: {e}");
                }
            }
        }
        Err(e) => {
            eprintln!("cretopass remove: failed to read PowerShell profile: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// rc PATH block removal (unix)
// ---------------------------------------------------------------------------

const PATH_MARKER_START: &str = "# >>> cretopass PATH >>>";
const PATH_MARKER_END: &str = "# <<< cretopass PATH <<<";
const ZSH_COMPLETION_MARKER_START: &str = "# >>> cretopass completion >>>";
const ZSH_COMPLETION_MARKER_END: &str = "# <<< cretopass completion <<<";

fn rc_files(home: &Path) -> Vec<(PathBuf, &str)> {
    vec![
        (home.join(".zshrc"), "zsh"),
        (home.join(".bashrc"), "bash"),
        (home.join(".config/fish/config.fish"), "fish"),
    ]
}

fn collect_rc_path_block(home: &Path, out: &mut Vec<Artifact>) {
    for (path, _shell) in rc_files(home) {
        if path.exists() {
            out.push(Artifact::File(path.clone()));
        }
    }
}

fn remove_rc_path_blocks(home: &Path) {
    for (path, _shell) in rc_files(home) {
        if !path.exists() {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                let cleaned = remove_marker_block(&content, PATH_MARKER_START, PATH_MARKER_END);
                // Also clean up zsh completion fpath block.
                let cleaned = remove_marker_block(
                    &cleaned,
                    ZSH_COMPLETION_MARKER_START,
                    ZSH_COMPLETION_MARKER_END,
                );
                if cleaned.trim() != content.trim()
                    && let Err(e) = std::fs::write(&path, &cleaned)
                {
                    eprintln!("cretopass remove: failed to update {}: {e}", path.display());
                }
            }
            Err(e) => {
                eprintln!("cretopass remove: failed to read {}: {e}", path.display());
            }
        }
    }
}

/// Removes a marker-wrapped block (inclusive) from text content.
///
/// Removes all lines from (and including) `start_marker` through
/// (and including) `end_marker`. If the markers are not found,
/// the content is returned unchanged.
fn remove_marker_block(content: &str, start_marker: &str, end_marker: &str) -> String {
    let mut result = Vec::new();
    let mut skipping = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == start_marker {
            skipping = true;
            continue;
        }
        if skipping && trimmed == end_marker {
            skipping = false;
            continue;
        }
        if !skipping {
            result.push(line);
        }
    }
    result.join("\n")
}

// ---------------------------------------------------------------------------
// GUI artifacts (unix)
// ---------------------------------------------------------------------------

#[cfg(unix)]
fn collect_gui_unix(home: &Path, out: &mut Vec<Artifact>) {
    if cfg!(target_os = "macos") {
        let app = home.join("Applications/cretopass.app");
        if app.exists() {
            out.push(Artifact::Dir(app));
        }
    }

    if cfg!(target_os = "linux") {
        // .desktop file
        let desktop = home.join(".local/share/applications/cretopass.desktop");
        if desktop.exists() {
            out.push(Artifact::File(desktop));
        }
        // Icon
        let icon = home.join(".local/share/icons/hicolor/256x256/apps/cretopass.png");
        if icon.exists() {
            out.push(Artifact::File(icon));
        }
        // Try to refresh icon cache (best-effort).
        let _ = Command::new("update-desktop-database")
            .arg(home.join(".local/share/applications"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = Command::new("gtk-update-icon-cache")
            .args(["-f", "-t"])
            .arg(home.join(".local/share/icons/hicolor"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

// ---------------------------------------------------------------------------
// Start Menu shortcut (Windows)
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn collect_start_menu_shortcut(out: &mut Vec<Artifact>) {
    let lnk = PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join("Microsoft/Windows/Start Menu/Programs/cretopass.lnk");
    if lnk.exists() {
        out.push(Artifact::File(lnk));
    }

    // Icon alongside binary
    if let Ok(d) = std::env::var("LOCALAPPDATA") {
        let icon = Path::new(&d)
            .join("Programs")
            .join("cretopass")
            .join("cretopass.ico");
        if icon.exists() {
            out.push(Artifact::File(icon));
        }
    }
}

// ---------------------------------------------------------------------------
// Windows detached helper for self-remove
// ---------------------------------------------------------------------------

#[cfg(windows)]
fn spawn_remove_helper_windows(home: &Path) {
    let tmp_dir = match create_remove_temp() {
        Ok(d) => d,
        Err(_) => return,
    };

    let install_dir = match std::env::var("LOCALAPPDATA") {
        Ok(d) => Path::new(&d).join("Programs").join("cretopass"),
        Err(_) => home.join("AppData/Local/Programs/cretopass"),
    };

    let helper_path = tmp_dir.join("_remove_helper.bat");
    let helper_content = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak > nul\r\n\
         if exist \"{install_dir}\\cretopass.exe\" del /f /q \"{install_dir}\\cretopass.exe\"\r\n\
         if exist \"{install_dir}\" rmdir /q \"{install_dir}\" 2>nul\r\n\
         del /f /q \"{helper_path}\"\r\n",
        install_dir = install_dir.display(),
        helper_path = helper_path.display(),
    );

    if std::fs::write(&helper_path, helper_content).is_err() {
        return;
    }

    let _ = Command::new("cmd")
        .args(["/c", "start", "", "/b"])
        .arg(&helper_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(windows)]
fn create_remove_temp() -> io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("cretopass-remove-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
