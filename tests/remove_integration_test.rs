//! End-to-end tests for `cretopass remove`.
//!
//! Drives the compiled `cretopass` binary with an isolated `$HOME` and
//! `$CRETOPASS_CONFIG_DIR`, prefilled with every artifact that `install.sh`
//! produces, then asserts nothing is left behind after removal.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Every artifact `install.sh` creates on Linux, relative to `$HOME`.
fn artifact_rel_paths() -> Vec<PathBuf> {
    vec![
        PathBuf::from(".local/bin/cretopass"),
        PathBuf::from(".local/share/bash-completion/completions/cretopass"),
        PathBuf::from(".zsh/completions/_cretopass"),
        PathBuf::from(".config/fish/completions/cretopass.fish"),
        PathBuf::from(".local/share/applications/cretopass.desktop"),
        PathBuf::from(".local/share/icons/hicolor/256x256/apps/cretopass.png"),
    ]
}

/// The rc files `install.sh` edits, with the marker text it inserts.
fn rc_fixtures() -> Vec<(PathBuf, &'static str)> {
    vec![
        (
            PathBuf::from(".bashrc"),
            "export PATH=\"$HOME/bin:$PATH\"\n# >>> cretopass PATH >>>\nexport PATH=\"$HOME/.local/bin:$PATH\"\n# <<< cretopass PATH <<<\nalias ll='ls -la'\n",
        ),
        (
            PathBuf::from(".zshrc"),
            "compinit\n# >>> cretopass completion >>>\nfpath=(\"$HOME/.zsh/completions\" $fpath)\n# <<< cretopass completion <<<\n# >>> cretopass PATH >>>\nexport PATH=\"$HOME/.local/bin:$PATH\"\n# <<< cretopass PATH <<<\n",
        ),
        (
            PathBuf::from(".config/fish/config.fish"),
            "set -gx PATH $HOME/bin $PATH\n# >>> cretopass PATH >>>\nfish_add_path $HOME/.local/bin\n# <<< cretopass PATH <<<\n",
        ),
    ]
}

/// Creates a fake home directory containing all artifacts and rc fixtures,
/// plus a copy of the real cretopass binary in `~/.local/bin` so that
/// `require_self_managed()` detects it.
fn setup_home(tmp: &Path) -> PathBuf {
    let home = tmp.join("home");
    fs::create_dir_all(&home).unwrap();

    for rel in artifact_rel_paths() {
        let path = home.join(&rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"fixture").unwrap();
    }

    // Config directory is a directory (not a file).
    let config_dir = home.join(".config/cretopass");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(config_dir.join("vault.dat"), b"secret").unwrap();

    for (rel, content) in rc_fixtures() {
        let path = home.join(&rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
    }

    // Copy the real binary into ~/.local/bin so `detect` sees a self-managed
    // install rooted under `$HOME`.
    fs::copy(
        env!("CARGO_BIN_EXE_cretopass"),
        home.join(".local/bin/cretopass"),
    )
    .unwrap();

    home
}

/// Runs `cretopass remove` inside `home`, answering the prompt with "y".
fn run_remove(home: &Path) -> std::process::ExitStatus {
    let mut child = Command::new(home.join(".local/bin/cretopass"))
        .arg("remove")
        .env("HOME", home)
        .env_remove("XDG_CONFIG_HOME")
        .env("CRETOPASS_CONFIG_DIR", home.join(".config/cretopass"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    child.stdin.take().unwrap().write_all(b"y\n").unwrap();
    child.wait().unwrap()
}

/// All file artifacts and the config directory are deleted.
#[test]
fn remove_deletes_all_file_artifacts_and_config_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let home = setup_home(tmp.path());

    let status = run_remove(&home);
    assert!(status.success(), "remove exited with {status}");

    for rel in artifact_rel_paths() {
        assert!(
            !home.join(&rel).exists(),
            "artifact {} was not removed",
            rel.display()
        );
    }
    assert!(
        !home.join(".config/cretopass").exists(),
        "config directory was not removed"
    );
}

/// rc files still exist but no longer contain any cretopass marker lines.
#[test]
fn remove_strips_markers_from_rc_files_but_keeps_them() {
    let tmp = tempfile::tempdir().unwrap();
    let home = setup_home(tmp.path());

    let status = run_remove(&home);
    assert!(status.success(), "remove exited with {status}");

    for (rel, _original) in rc_fixtures() {
        let path = home.join(&rel);
        assert!(path.exists(), "rc file {} was deleted", rel.display());

        let content = fs::read_to_string(&path).unwrap();
        assert!(
            !content.contains("cretopass PATH") && !content.contains("cretopass completion"),
            "markers remain in {}:\n{content}",
            rel.display()
        );
    }

    // Lines that live *outside* the marker blocks must survive verbatim.
    let surviving_lines = [
        (".bashrc", "export PATH=\"$HOME/bin:$PATH\""),
        (".bashrc", "alias ll='ls -la'"),
        (".zshrc", "compinit"),
        (".config/fish/config.fish", "set -gx PATH $HOME/bin $PATH"),
    ];
    for (rel, expected) in surviving_lines {
        let content = fs::read_to_string(home.join(rel)).unwrap();
        assert!(
            content.contains(expected),
            "lost expected line in {rel}: {expected}\n---\n{content}"
        );
    }
}

/// A home with nothing installed reports "no artifacts" and exits 0.
#[test]
fn remove_on_clean_home_is_a_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("empty_home");
    fs::create_dir_all(home.join(".local/bin")).unwrap();
    fs::copy(
        env!("CARGO_BIN_EXE_cretopass"),
        home.join(".local/bin/cretopass"),
    )
    .unwrap();

    let status = run_remove(&home);
    assert!(status.success(), "remove exited with {status}");
}
