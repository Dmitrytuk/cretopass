use std::path::Path;
use std::process::{Command, ExitCode, Stdio};

#[cfg(windows)]
use std::io;

use super::detect::InstallSource;
use super::layout;
use super::layout::TempDir;

const REPO: &str = "Dmitrytuk/cretopass";
#[cfg(unix)]
const INSTALLER: &str = "install.sh";
#[cfg(windows)]
const INSTALLER: &str = "install.ps1";

/// Runs the `cretopass update` subcommand.
///
/// Downloads the platform installer script from GitHub Releases and runs it.
/// On Windows, the installer is run via a detached helper script after the
/// current process exits to avoid file-locking issues; that path reports
/// staging success (not the installer's result), which is an accepted
/// asymmetry documented in ADR 0008.
pub(crate) fn run_update() -> ExitCode {
    if let Err(err) = InstallSource::detect().require_self_managed() {
        eprintln!("cretopass update: {err}");
        return ExitCode::from(super::EXIT_FOREIGN_INSTALL);
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
    let url = match installer_url() {
        Ok(url) => url,
        Err(e) => {
            eprintln!("cretopass update: {e}");
            return ExitCode::FAILURE;
        }
    };

    let tmp = match TempDir::create("cretopass-update") {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("cretopass update: failed to create temp directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let script_path = tmp.path().join(INSTALLER);
    if let Err(e) = layout::create_new_file(&script_path) {
        eprintln!("cretopass update: failed to stage installer file: {e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = download_with_curl(&url, &script_path) {
        eprintln!("cretopass update: failed to download installer: {e}");
        return ExitCode::FAILURE;
    }

    let status = Command::new("sh")
        .arg(&script_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    // `tmp` is dropped here, removing the staged script and directory.
    match status {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => exit_code_from(s.code().unwrap_or(1)),
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

    let url = match installer_url() {
        Ok(url) => url,
        Err(e) => {
            eprintln!("cretopass update: {e}");
            return ExitCode::FAILURE;
        }
    };

    let tmp = match TempDir::create("cretopass-update") {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("cretopass update: failed to create temp directory: {e}");
            return ExitCode::FAILURE;
        }
    };

    let script_path = tmp.path().join(INSTALLER);
    if let Err(e) = layout::create_new_file(&script_path) {
        eprintln!("cretopass update: failed to stage installer file: {e}");
        return ExitCode::FAILURE;
    }
    if let Err(e) = download_with_powershell(&url, &script_path) {
        eprintln!("cretopass update: failed to download installer: {e}");
        return ExitCode::FAILURE;
    }

    // Write a detached helper batch script that waits for the current
    // process to exit, then runs the installer with -Force.
    let helper_path = tmp.join("_helper.bat");
    if let Err(e) = write_update_helper(&helper_path, &script_path, tmp.path()) {
        eprintln!("cretopass update: failed to write helper script: {e}");
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
            // The detached helper reads the staged script after this process
            // exits, so the temp directory must outlive the drop guard.
            tmp.keep();
            println!("Update staged. It will complete automatically after this window closes.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("cretopass update: failed to start update helper: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Writes the detached Windows update helper.
///
/// Every embedded path is validated to be absolute and free of shell
/// metacharacters before it is interpolated, so an attacker-controlled
/// `TEMP`/`TMP`/`LOCALAPPDATA` value cannot inject commands.
#[cfg(windows)]
fn write_update_helper(helper: &Path, script: &Path, tmp: &Path) -> Result<(), String> {
    for path in [helper, script, tmp] {
        layout::ensure_safe_windows_path(path)?;
    }

    let content = format!(
        "@echo off\r\n\
         timeout /t 2 /nobreak > nul\r\n\
         powershell -NoProfile -ExecutionPolicy Bypass -File \"{script}\" -Force\r\n\
         del /f /q \"{script}\"\r\n\
         rmdir /q /s \"{tmp}\"\r\n",
        script = script.display(),
        tmp = tmp.display(),
    );
    std::fs::write(helper, content).map_err(|e| format!("failed to write helper: {e}"))
}

/// Returns the URL of the installer script to download.
///
/// Honours the `CRETOPASS_UPDATE_URL` environment variable in debug builds to
/// allow tests and mirrors to point at a different source. The override is
/// compiled out of release builds. An empty value is treated as unset.
fn installer_url() -> Result<String, String> {
    match update_url_override() {
        Some(url) => {
            validate_update_url(&url)?;
            Ok(url)
        }
        None => Ok(default_installer_url()),
    }
}

/// Reads the debug-only `CRETOPASS_UPDATE_URL` override.
#[cfg(debug_assertions)]
fn update_url_override() -> Option<String> {
    std::env::var("CRETOPASS_UPDATE_URL")
        .ok()
        .filter(|value| !value.is_empty())
}

/// Release builds ignore the override entirely.
#[cfg(not(debug_assertions))]
fn update_url_override() -> Option<String> {
    None
}

/// The default GitHub Releases URL for the current platform's installer.
fn default_installer_url() -> String {
    installer_url_from(None)
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

/// Rejects an update URL that could be parsed as a `curl` option or an
/// unsupported protocol.
fn validate_update_url(url: &str) -> Result<(), String> {
    if url.starts_with('-') {
        return Err(format!("refusing update URL starting with '-': {url}"));
    }
    if !scheme_allowed(url) {
        return Err(format!("refusing unsupported update URL scheme: {url}"));
    }
    Ok(())
}

/// Debug builds also accept `file://` fixtures.
#[cfg(debug_assertions)]
fn scheme_allowed(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("file://")
}

/// Release builds accept `https://` only.
#[cfg(not(debug_assertions))]
fn scheme_allowed(url: &str) -> bool {
    url.starts_with("https://")
}

/// Maps a child exit status into an [`ExitCode`].
///
/// Out-of-range or signal-terminated statuses become failure rather than being
/// truncated into a misleading success (`256 as u8 == 0`).
#[cfg(unix)]
fn exit_code_from(code: i32) -> ExitCode {
    match u8::try_from(code) {
        Ok(code) => ExitCode::from(code),
        Err(_) => ExitCode::FAILURE,
    }
}

/// Downloads a file using `curl`.
///
/// The protocol is pinned to HTTPS (with no redirect downgrade) and the URL is
/// placed after `--` so a value can never be parsed as a curl option.
#[cfg(unix)]
fn download_with_curl(url: &str, dest: &Path) -> Result<(), String> {
    let mut command = Command::new("curl");
    command.arg("-fsSL").arg("-o").arg(dest);
    if url.starts_with("https://") {
        command.args(["--proto", "=https", "--proto-redir", "=https"]);
    }
    let status = command
        .arg("--")
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
///
/// The URL and destination are passed through the environment rather than
/// interpolated into a `-Command` string, so a quote in either value cannot
/// break out into arbitrary PowerShell.
#[cfg(windows)]
fn download_with_powershell(url: &str, dest: &Path) -> Result<(), String> {
    let status = Command::new("powershell")
        .env("CRETOPASS_DL_URL", url)
        .env("CRETOPASS_DL_DEST", dest)
        .args([
            "-NoProfile",
            "-Command",
            "Invoke-WebRequest -Uri $env:CRETOPASS_DL_URL -OutFile $env:CRETOPASS_DL_DEST -UseBasicParsing",
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

    /// A leading `-` would be parsed by `curl` as an option, so it is refused.
    #[test]
    fn validate_update_url_rejects_option_like_values() {
        let err = validate_update_url("-Kconfig").unwrap_err();
        assert!(err.contains("starting with '-'"), "{err}");
    }

    /// A plain-HTTP URL is refused.
    #[test]
    fn validate_update_url_rejects_plain_http() {
        assert!(validate_update_url("http://example.com/install.sh").is_err());
    }

    /// HTTPS is always accepted.
    #[test]
    fn validate_update_url_accepts_https() {
        assert!(validate_update_url("https://example.com/install.sh").is_ok());
    }

    /// Debug builds accept a `file://` fixture URL so tests can run offline.
    #[test]
    #[cfg(debug_assertions)]
    fn validate_update_url_accepts_file_in_debug() {
        assert!(validate_update_url("file:///tmp/fake-install.sh").is_ok());
    }

    /// Exit codes inside the `u8` range are preserved exactly.
    #[test]
    #[cfg(unix)]
    fn exit_code_from_preserves_in_range_codes() {
        assert_eq!(exit_code_from(0), ExitCode::SUCCESS);
        assert_eq!(exit_code_from(1), ExitCode::from(1));
        assert_eq!(exit_code_from(255), ExitCode::from(255));
    }

    /// Out-of-range statuses must not truncate into a false success.
    #[test]
    #[cfg(unix)]
    fn exit_code_from_rejects_out_of_range() {
        assert_eq!(exit_code_from(256), ExitCode::FAILURE);
        assert_eq!(exit_code_from(-1), ExitCode::FAILURE);
    }

    /// `download_with_curl` copies the source at a `file://` URL to the
    /// destination path. Skips at runtime when `curl` is unavailable.
    #[cfg(unix)]
    #[test]
    fn download_with_curl_file_url_copies_content() {
        if !curl_available() {
            eprintln!("skipping: curl is not available");
            return;
        }

        let tmp = TempDir::create("cretopass-curl-test").unwrap();
        let fixture = tmp.path().join("fixture.sh");
        let dest = tmp.path().join("dest.sh");
        let content = "#!/bin/sh\necho cretopass-test\n";
        std::fs::write(&fixture, content).expect("write fixture");

        let url = format!("file://{}", fixture.display());
        download_with_curl(&url, &dest).expect("curl download should succeed");

        assert_eq!(std::fs::read_to_string(&dest).unwrap(), content);
    }

    /// Returns true when `curl` is on `PATH`.
    #[cfg(unix)]
    fn curl_available() -> bool {
        Command::new("curl")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
