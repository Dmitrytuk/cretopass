use std::io;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

const REPO: &str = "Dmitrytuk/cretopass";
#[cfg(unix)]
const INSTALLER: &str = "install.sh";
#[cfg(windows)]
const INSTALLER: &str = "install.ps1";

/// Runs the `cretopass update` subcommand.
///
/// Downloads the platform installer script from GitHub Releases and runs it.
/// On Windows, the installer is run via a detached helper script after the
/// current process exits to avoid file-locking issues.
pub fn run_update() -> ExitCode {
    if let Err(msg) = super::detect::InstallSource::detect().require_self_managed() {
        eprintln!("cretopass update: {msg}");
        return ExitCode::from(2);
    }

    #[cfg(windows)]
    {
        run_update_windows()
    }
    #[cfg(unix)]
    {
        run_update_unix()
    }
}

#[cfg(unix)]
fn run_update_unix() -> ExitCode {
    let tmp_dir = match create_temp_dir("cretopass-update") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cretopass update: failed to create temp directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let script_path = tmp_dir.join(INSTALLER);
    let url = installer_url();

    if let Err(e) = download_with_curl(&url, &script_path) {
        eprintln!("cretopass update: failed to download installer: {e}");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return ExitCode::FAILURE;
    }

    let status = Command::new("sh")
        .arg(&script_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    let _ = std::fs::remove_dir_all(&tmp_dir);

    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            ExitCode::from(code as u8)
        }
        Err(e) => {
            eprintln!("cretopass update: failed to run installer: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(windows)]
fn run_update_windows() -> ExitCode {
    // Prompt to close other instances of cretopass before updating.
    println!("cretopass must not be running during the update.");
    println!("Close any open cretopass windows, then press Enter to continue (Ctrl+C to cancel).");
    let mut input = String::new();
    if io::stdin().read_line(&mut input).is_err() {
        eprintln!("cretopass update: failed to read input");
        return ExitCode::FAILURE;
    }

    let tmp_dir = match create_temp_dir("cretopass-update") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cretopass update: failed to create temp directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let script_path = tmp_dir.join(INSTALLER);
    let url = installer_url();

    if let Err(e) = download_with_powershell(&url, &script_path) {
        eprintln!("cretopass update: failed to download installer: {e}");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return ExitCode::FAILURE;
    }

    // Write a detached helper batch script that waits for the current
    // process to exit, then runs the installer with -Force.
    let helper_path = tmp_dir.join("_helper.bat");
    let helper_content = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak > nul\r\n\
         powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\" -Force\r\n\
         del /f /q \"{}\"\r\n",
        script_path.display(),
        helper_path.display(),
    );
    if let Err(e) = std::fs::write(&helper_path, helper_content) {
        eprintln!("cretopass update: failed to write helper script: {e}");
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return ExitCode::FAILURE;
    }

    // Spawn the helper detached — no console window, no waiting.
    let spawn_result = Command::new("cmd")
        .args(["/c", "start", "", "/b"])
        .arg(&helper_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();

    match spawn_result {
        Ok(_) => {
            println!("Update staged. It will complete automatically after this window closes.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cretopass update: failed to start update helper: {e}");
            let _ = std::fs::remove_dir_all(&tmp_dir);
            ExitCode::FAILURE
        }
    }
}

/// Returns the URL of the installer script to download.
///
/// Honours the `CRETOPASS_UPDATE_URL` environment variable to allow tests (and
/// mirrors) to point at a different source. An empty value is treated as unset.
fn installer_url() -> String {
    installer_url_from(
        std::env::var("CRETOPASS_UPDATE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .as_deref(),
    )
}

/// Resolves the installer URL from an optional override.
///
/// Returns `base` unchanged when provided, otherwise the default GitHub
/// Releases URL for the current platform's installer.
fn installer_url_from(base: Option<&str>) -> String {
    match base {
        Some(url) => url.to_string(),
        None => format!("https://github.com/{REPO}/releases/latest/download/{INSTALLER}"),
    }
}

/// Creates a uniquely named temporary directory.
fn create_temp_dir(prefix: &str) -> io::Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Downloads a file using `curl`.
#[cfg(unix)]
fn download_with_curl(url: &str, dest: &PathBuf) -> Result<(), String> {
    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(dest)
        .arg(url)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to run curl: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("curl exited with status {status}"))
    }
}

/// Downloads a file using PowerShell's `Invoke-WebRequest`.
#[cfg(windows)]
fn download_with_powershell(url: &str, dest: &std::path::Path) -> Result<(), String> {
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Invoke-WebRequest -Uri '{url}' -OutFile '{}' -UseBasicParsing",
                dest.display()
            ),
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| format!("failed to run powershell: {e}"))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("powershell exited with status {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Without an override, the installer URL points at the default GitHub
    /// Releases location for this platform's installer.
    #[test]
    fn installer_url_from_none_defaults_to_github() {
        let expected = format!("https://github.com/{REPO}/releases/latest/download/{INSTALLER}");
        assert_eq!(installer_url_from(None), expected);
    }

    /// A provided override is returned verbatim (e.g. a `file://` fixture URL).
    #[test]
    fn installer_url_from_some_uses_provided_url() {
        assert_eq!(
            installer_url_from(Some("file:///tmp/fake-install.sh")),
            "file:///tmp/fake-install.sh"
        );
    }

    /// `download_with_curl` copies the source at a `file://` URL to the
    /// destination path. Requires `curl` on `PATH`.
    #[cfg(unix)]
    #[test]
    fn download_with_curl_file_url_copies_content() {
        let fixture = std::env::temp_dir().join("cretopass-test-fixture.sh");
        let dest = std::env::temp_dir().join("cretopass-test-dest.sh");
        let content = "#!/bin/sh\necho cretopass-test\n";
        std::fs::write(&fixture, content).expect("write fixture");

        let url = format!("file://{}", fixture.display());
        let result = download_with_curl(&url, &dest);

        let copied = std::fs::read_to_string(&dest).ok();

        let _ = std::fs::remove_file(&fixture);
        let _ = std::fs::remove_file(&dest);

        result.expect("curl download should succeed");
        assert_eq!(copied.as_deref(), Some(content));
    }
}
