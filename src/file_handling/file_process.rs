use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use super::FileHandlingError;
use super::atomic;
use crate::crypt;
use crate::theming::texts;
use crate::tree::node::Node;

/// The maximum size (in bytes) of a vault or import the gateway will read.
pub(crate) const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
/// The age file-format magic header every encrypted vault begins with.
const AGE_MAGIC: &[u8; 11] = b"age-encrypt";
/// Mode (Unix) directories the gateway creates are given: owner only.
const SECRET_DIR_MODE: u32 = 0o700;

/// A single entry in a directory listing.
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// The entry's file name (lossy for a non-UTF-8 name).
    pub name: String,
    /// The entry's full path.
    pub path: PathBuf,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Whether the entry is a regular file that carries the age magic header.
    pub is_age_file: bool,
}

// ── JSON helpers ──

/// Serialises `node` to pretty-printed JSON.
///
/// The result holds every secret in the tree in the clear; the caller is
/// responsible for zeroizing it.
///
/// # Errors
///
/// Returns a [`serde_json::Error`] when `node` cannot be serialised.
pub fn node_to_json(node: &Node) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(node)
}

/// Parses `json` into a [`Node`], migrating a legacy top-level `description`.
///
/// # Errors
///
/// Returns a [`serde_json::Error`] when `json` is not a valid node.
pub fn json_to_node(json: &str) -> Result<Node, serde_json::Error> {
    serde_json::from_str(json)
}

// ── Raw I/O ──

/// Reads the whole of `path` into memory, refusing to follow a symlink.
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when `path` cannot be read, is a symlink
/// (Unix), or is not a regular file.
pub fn read_file(path: &str) -> Result<Vec<u8>, FileHandlingError> {
    let mut file = atomic::open_for_read(Path::new(path))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(FileHandlingError::Io)?;
    Ok(data)
}

/// Writes `data` to `path` atomically.
///
/// Every write goes through the same exclusive temp file + `fsync` + rename
/// path as a vault save, so a crash mid-write can never leave a truncated file.
/// When `path` already exists as a regular file its mode is preserved; a file
/// this creates is owner-only on Unix.
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when `path` cannot be written.
pub fn write_file(path: &str, data: &[u8]) -> Result<(), FileHandlingError> {
    atomic::write_atomic(Path::new(path), data)
}

// ── Encrypted file operations ──

/// Validates and reads an age-encrypted file.  Returns the full bytes.
///
/// # Errors
///
/// Returns [`FileHandlingError::FileTooLarge`] when the file exceeds the shared
/// size cap, [`FileHandlingError::FileTooSmall`] when it is shorter than the
/// magic header, [`FileHandlingError::NotAnAgeFile`] when the header does not
/// match, and [`FileHandlingError::Io`] when the file cannot be opened or read.
pub fn validate_encrypted_file(path: &str) -> Result<Vec<u8>, FileHandlingError> {
    let file = atomic::open_for_read(Path::new(path))?;
    let meta = file.metadata().map_err(FileHandlingError::Io)?;

    if meta.len() > MAX_FILE_SIZE {
        return Err(FileHandlingError::FileTooLarge {
            size: meta.len(),
            max: MAX_FILE_SIZE,
        });
    }

    if meta.len() < AGE_MAGIC.len() as u64 {
        return Err(FileHandlingError::FileTooSmall);
    }

    // Read through a bounded reader so a file that grows (or is swapped for a
    // stream) after the metadata check cannot exhaust memory.
    let mut data = Vec::new();
    file.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut data)
        .map_err(FileHandlingError::Io)?;

    if data.len() as u64 > MAX_FILE_SIZE {
        return Err(FileHandlingError::FileTooLarge {
            size: data.len() as u64,
            max: MAX_FILE_SIZE,
        });
    }

    if !data.starts_with(AGE_MAGIC) {
        return Err(FileHandlingError::NotAnAgeFile);
    }

    Ok(data)
}

/// Decrypts age-encrypted bytes with a passphrase and deserialises into a Node.
///
/// # Errors
///
/// Returns [`FileHandlingError::Crypt`] when decryption fails and
/// [`FileHandlingError::Serde`] when the plaintext is not a Node.
pub fn decrypt_to_node(data: &[u8], passphrase: &str) -> Result<Node, FileHandlingError> {
    // Keep the decrypted plaintext zeroized when it is dropped.
    let json = Zeroizing::new(crypt::decrypt(data, passphrase)?);
    json_to_node(&json).map_err(FileHandlingError::Serde)
}

/// Serialises `node` to JSON (zeroizing the plaintext buffer afterwards) and
/// encrypts it with `passphrase`.
fn encrypt_node(node: &Node, passphrase: &str) -> Result<Vec<u8>, FileHandlingError> {
    let json = Zeroizing::new(node_to_json(node)?);
    Ok(crypt::encrypt(&json, passphrase)?)
}

/// Saves a Node tree to an age-encrypted file with an atomic write.
///
/// The new ciphertext is written to a temporary file in the same directory,
/// flushed and `sync`ed, then atomically renamed over the target path, and the
/// containing directory is `sync`ed, so a crash mid-write never leaves a
/// truncated or corrupt vault. The temp file is created exclusively (never
/// following a planted symlink) at mode `0o600`; when the target already
/// exists its previous mode is preserved.
///
/// # Errors
///
/// Returns [`FileHandlingError::InvalidPassphraseEncoding`] when `passphrase`
/// is not UTF-8, [`FileHandlingError::Serde`] when `root` cannot be
/// serialised, [`FileHandlingError::Crypt`] when encryption fails, and
/// [`FileHandlingError::Io`] when the temp file cannot be written or renamed.
pub fn save_tree(root: &Node, path: &str, passphrase: &[u8]) -> Result<(), FileHandlingError> {
    let passphrase_str = std::str::from_utf8(passphrase)
        .map_err(|_| FileHandlingError::InvalidPassphraseEncoding)?;

    let encrypted = encrypt_node(root, passphrase_str)?;

    atomic::write_atomic(Path::new(path), &encrypted)
}

/// Creates a new encrypted secret file at `path` with a "Secrets" root node.
///
/// The file is created by a single no-clobber rename, so an existing file (or a
/// symlink) is never overwritten, no pre-existing vault can be destroyed, and a
/// crash at any point leaves either the previous file or nothing — never a
/// zero-byte placeholder.
///
/// # Errors
///
/// Returns [`FileHandlingError::PathNotWritable`] when `path` cannot be
/// created (for example a missing parent directory), and
/// [`FileHandlingError::Io`] with [`std::io::ErrorKind::AlreadyExists`] when
/// it already exists. Encryption and serialisation failures surface as
/// [`FileHandlingError::Crypt`] / [`FileHandlingError::Serde`].
pub fn create_secret_file(path: &str, passphrase: &str) -> Result<Node, FileHandlingError> {
    let node = Node::new(texts::DEFAULT_ROOT_NAME);
    let data = encrypt_node(&node, passphrase)?;

    // `write_atomic_new` can only fail with an `Io` error (encryption has
    // already succeeded). A lost race maps to `AlreadyExists`; every other
    // failure — a missing parent, a read-only directory, a full disk, a
    // filesystem that supports neither a no-clobber rename nor a hard link —
    // means the path could not be written.
    atomic::write_atomic_new(Path::new(path), &data).map_err(|error| match error {
        FileHandlingError::Io(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            FileHandlingError::Io(e)
        }
        _ => FileHandlingError::PathNotWritable(path.to_string()),
    })?;

    Ok(node)
}

// ── Filesystem operations ──

/// Lists all visible entries in `path`, sorted directories-first then
/// alphabetically.  Hidden (dot-prefixed) entries are skipped.
///
/// Each regular file is scanned for the age-encryption magic header.
pub fn list_dir(path: &Path) -> Result<Vec<FileEntry>, FileHandlingError> {
    list_dir_impl(path, false)
}

/// Lists all entries in `path`, including hidden (dot-prefixed) entries,
/// sorted directories-first then alphabetically.
///
/// Each regular file is scanned for the age-encryption magic header.
pub fn list_dir_including_hidden(path: &Path) -> Result<Vec<FileEntry>, FileHandlingError> {
    list_dir_impl(path, true)
}

fn list_dir_impl(path: &Path, include_hidden: bool) -> Result<Vec<FileEntry>, FileHandlingError> {
    let mut entries: Vec<FileEntry> = Vec::new();

    let read_dir = fs::read_dir(path)?;

    for entry in read_dir {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !include_hidden && name.starts_with('.') {
            continue;
        }

        let path = entry.path();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        // `is_file` follows symlinks and rejects special files (a FIFO open
        // would otherwise block the event-loop thread): keep the guard.
        let is_age_file = !is_dir && path.is_file() && scan_age_header(&path);

        entries.push(FileEntry {
            name,
            path,
            is_dir,
            is_age_file,
        });
    }

    // Directories first (false sorts before true), then case-insensitively by
    // name, computing each lowercased key once.
    entries.sort_by_cached_key(|entry| (!entry.is_dir, entry.name.to_lowercase()));

    Ok(entries)
}

/// Returns `true` if the file starts with the age-encryption magic header.
pub fn is_age_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    scan_age_header(path)
}

fn scan_age_header(path: &Path) -> bool {
    let mut file = match atomic::open_for_read(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut header = [0u8; AGE_MAGIC.len()];
    if file.read_exact(&mut header).is_err() {
        return false;
    }
    &header == AGE_MAGIC
}

/// Creates `path` and any missing parents, for directories the user asked to
/// create (for example the File Browser's "New Folder").
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when a directory cannot be created.
pub fn create_dir(path: &Path) -> Result<(), FileHandlingError> {
    fs::create_dir_all(path).map_err(FileHandlingError::Io)
}

/// Creates `path` and any missing parents with owner-only permissions on Unix,
/// for directories that hold secrets (share bundles, the config directory).
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when a directory cannot be created.
pub(crate) fn create_private_dir(path: &Path) -> Result<(), FileHandlingError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(SECRET_DIR_MODE);
    }
    builder.create(path).map_err(FileHandlingError::Io)
}

/// Deletes the file at `path`.
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when `path` cannot be removed.
pub fn delete_file(path: &Path) -> Result<(), FileHandlingError> {
    fs::remove_file(path)?;
    Ok(())
}

/// Renames `from` to `to`, replacing `to` if it exists.
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when the rename fails.
pub fn rename_file(from: &Path, to: &Path) -> Result<(), FileHandlingError> {
    fs::rename(from, to)?;
    Ok(())
}

/// Checks whether a path is writable by creating and removing an exclusive
/// temp file in its directory (never following a planted symlink).
///
/// # Errors
///
/// Returns [`FileHandlingError::PathNotWritable`] when no temp file can be
/// created beside `path`.
pub fn probe_path_writable(path: &str) -> Result<(), FileHandlingError> {
    atomic::probe_writable(Path::new(path))
        .map_err(|_| FileHandlingError::PathNotWritable(path.to_string()))
}

/// Returns `true` when `path` is a directory with no entries at all, including
/// hidden ones (dot-prefixed).
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when the directory cannot be read.
pub fn is_dir_empty(path: &Path) -> Result<bool, FileHandlingError> {
    let mut entries = fs::read_dir(path)?;
    Ok(entries.next().is_none())
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::FileHandlingError;
    use super::*;
    use crate::crypt::CryptError;
    use crate::tree::node::{ExtraField, FieldType};

    // ── JSON roundtrip tests (original) ──

    #[test]
    fn test_json_roundtrip() {
        let mut node = Node::with_children("Root", vec![Node::new("Child1"), Node::new("Child2")]);
        node.extra_fields = vec![ExtraField::description("description")];
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.id, decoded.id);
        assert_eq!(node.name, decoded.name);
        assert_eq!(node.extra_fields.len(), decoded.extra_fields.len());
        assert_eq!(node.extra_fields[0].value, decoded.extra_fields[0].value);
        assert_eq!(node.children.len(), decoded.children.len());
    }

    #[test]
    fn test_json_roundtrip_empty() {
        let node = Node::new("");
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.id, decoded.id);
        assert_eq!(node.name, decoded.name);
        assert!(decoded.children.is_empty());
        assert!(decoded.extra_fields.is_empty());
    }

    #[test]
    fn test_json_roundtrip_migrates_legacy_description() {
        // A vault written before the description became an extra field.
        let json = r#"{"id":"00000000-0000-0000-0000-000000000000",
            "name":"Legacy","description":"old desc","extra_fields":[],"children":[]}"#;
        let decoded: Node = json_to_node(json).unwrap();
        assert_eq!(decoded.extra_fields.len(), 1);
        assert_eq!(decoded.extra_fields[0].value, "old desc");
        // Saving again drops the legacy key entirely.
        let reencoded = node_to_json(&decoded).unwrap();
        assert!(!reencoded.contains("\"description\""));
    }

    #[test]
    fn test_json_roundtrip_with_extra() {
        let mut node = Node::new("Entry");
        node.extra_fields = vec![
            ExtraField::description("Login credentials"),
            ExtraField {
                field_type: FieldType::Login,
                value: "admin".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "secret123".to_string(),
            },
            ExtraField {
                field_type: FieldType::Custom("Note".to_string()),
                value: "important".to_string(),
            },
        ];
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.extra_fields.len(), decoded.extra_fields.len());
        for (a, b) in node.extra_fields.iter().zip(decoded.extra_fields.iter()) {
            assert_eq!(a.value, b.value);
            match (&a.field_type, &b.field_type) {
                (FieldType::Login, FieldType::Login) => {}
                (FieldType::Password, FieldType::Password) => {}
                (FieldType::Custom(an), FieldType::Custom(bn)) => assert_eq!(an, bn),
                _ => panic!("field type mismatch"),
            }
        }
    }

    #[test]
    fn test_json_roundtrip_nested() {
        let node = Node::with_children(
            "Root",
            vec![
                Node::with_children("Category", vec![Node::new("Item1"), Node::new("Item2")]),
                Node::new("Sibling"),
            ],
        );
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.id, decoded.id);
        assert_eq!(node.children.len(), decoded.children.len());
        let cat = &decoded.children[0];
        assert_eq!(cat.children.len(), 2);
    }

    #[test]
    fn test_invalid_json() {
        let result = json_to_node("this is not valid json");
        assert!(result.is_err());

        let result = json_to_node(r#"{"name": "incomplete"}"#);
        assert!(result.is_err());
    }

    // ── Encrypted file tests ──

    fn create_test_encrypted_file(path: &str, passphrase: &str, node: &Node) {
        let json = node_to_json(node).unwrap();
        let encrypted = crypt::encrypt(&json, passphrase).unwrap();
        fs::write(path, &encrypted).unwrap();
    }

    #[test]
    fn test_validate_rejects_large_file() {
        let path = format!("/tmp/test_large_{}.bin", uuid::Uuid::new_v4());
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(file);

        let result = validate_encrypted_file(&path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FileHandlingError::FileTooLarge { .. }
        ));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_rejects_non_age_file() {
        let path = format!("/tmp/test_not_age_{}.txt", uuid::Uuid::new_v4());
        fs::write(&path, b"this is not an age file").unwrap();

        let result = validate_encrypted_file(&path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FileHandlingError::NotAnAgeFile
        ));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_accepts_valid_file() {
        let path = format!("/tmp/test_valid_{}.age", uuid::Uuid::new_v4());
        let node = Node::new("Test");
        create_test_encrypted_file(&path, "test-pass", &node);

        let result = validate_encrypted_file(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(b"age-encrypt"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_rejects_too_small() {
        let path = format!("/tmp/test_tiny_{}.bin", uuid::Uuid::new_v4());
        fs::write(&path, b"tiny").unwrap();

        let result = validate_encrypted_file(&path);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FileHandlingError::FileTooSmall
        ));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_decrypt_to_node_roundtrip() {
        let path = format!("/tmp/test_decrypt_{}.age", uuid::Uuid::new_v4());
        let original = Node::with_children("Root", vec![Node::new("Child1")]);
        create_test_encrypted_file(&path, "my-pass", &original);

        let data = fs::read(&path).unwrap();
        let decoded = decrypt_to_node(&data, "my-pass").unwrap();

        assert_eq!(original.id, decoded.id);
        assert_eq!(original.name, decoded.name);
        assert_eq!(original.children.len(), decoded.children.len());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_decrypt_to_node_wrong_passphrase() {
        let path = format!("/tmp/test_bad_pass_{}.age", uuid::Uuid::new_v4());
        let node = Node::new("Test");
        create_test_encrypted_file(&path, "correct-pass", &node);

        let data = fs::read(&path).unwrap();
        let result = decrypt_to_node(&data, "wrong-pass");
        assert!(matches!(
            result,
            Err(FileHandlingError::Crypt(CryptError::WrongPassphrase))
        ));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_decrypt_to_node_with_invalid_json_payload() {
        let path = format!("/tmp/test_bad_json_{}.age", uuid::Uuid::new_v4());
        // Encrypt bytes that decrypt fine but do not parse as a Node.
        let encrypted = crypt::encrypt("not a node", "my-pass").unwrap();
        fs::write(&path, &encrypted).unwrap();

        let data = fs::read(&path).unwrap();
        let result = decrypt_to_node(&data, "my-pass");
        assert!(matches!(result, Err(FileHandlingError::Serde(_))));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_tree_writes_over_the_previous_file() {
        let path = format!("/tmp/test_save_{}.age", uuid::Uuid::new_v4());
        let passphrase = b"save-test-pass";
        let passphrase_str = "save-test-pass";

        let initial = Node::new("Initial");
        create_test_encrypted_file(&path, passphrase_str, &initial);

        let mutated = Node::with_children("Updated Root", vec![Node::new("New Child")]);
        save_tree(&mutated, &path, passphrase).unwrap();

        // The atomic save leaves no transient artifact behind.
        assert!(fs::metadata(format!("{path}.bak")).is_err());

        let data = fs::read(&path).unwrap();
        let decoded = decrypt_to_node(&data, passphrase_str).unwrap();
        assert_eq!(decoded.name, "Updated Root");
        assert_eq!(decoded.children.len(), 1);
        assert_eq!(decoded.children[0].name, "New Child");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_tree_with_extra_fields() {
        let path = format!("/tmp/test_save_extra_{}.age", uuid::Uuid::new_v4());
        let passphrase = b"extra-test";
        let passphrase_str = "extra-test";

        let mut node = Node::new("Entry");
        node.extra_fields = vec![
            ExtraField::description("has extra fields"),
            ExtraField {
                field_type: FieldType::Login,
                value: "admin".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "secret".to_string(),
            },
        ];

        save_tree(&node, &path, passphrase).unwrap();

        let data = fs::read(&path).unwrap();
        let decoded = decrypt_to_node(&data, passphrase_str).unwrap();
        assert_eq!(decoded.extra_fields.len(), 3);
        assert_eq!(decoded.extra_fields[1].value, "admin");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_tree_creates_a_new_file() {
        let path = format!("/tmp/test_no_backup_{}.age", uuid::Uuid::new_v4());
        let _ = fs::remove_file(&path);

        let node = Node::new("New");
        save_tree(&node, &path, b"pass").unwrap();

        assert!(fs::metadata(&path).is_ok());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_create_secret_file_ok() {
        let path = format!("/tmp/test_create_ok_{}.age", uuid::Uuid::new_v4());
        let _ = fs::remove_file(&path);

        let node = create_secret_file(&path, "test-pass").unwrap();

        assert_eq!(node.name, "Secrets");
        assert!(node.children.is_empty());

        assert!(fs::metadata(&path).is_ok());
        let data = fs::read(&path).unwrap();
        assert!(data.starts_with(b"age-encrypt"));

        let json = crypt::decrypt(&data, "test-pass").unwrap();
        let decoded: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.name, "Secrets");
        assert!(decoded.children.is_empty());
        assert_eq!(node.id, decoded.id);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_create_secret_file_invalid_path() {
        let path = "/nonexistent_dir_12345/test.age".to_string();
        let result = create_secret_file(&path, "pass");
        assert!(matches!(result, Err(FileHandlingError::PathNotWritable(_))));
    }

    #[test]
    fn create_secret_file_refuses_to_overwrite_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        let path = path.to_string_lossy().to_string();

        let node = create_secret_file(&path, "pw").unwrap();
        assert_eq!(node.name, "Secrets");

        let result = create_secret_file(&path, "other");

        assert!(
            matches!(result, Err(FileHandlingError::Io(ref e))
                if e.kind() == std::io::ErrorKind::AlreadyExists),
            "a second create must refuse to clobber the first"
        );
        // The vault it refused to clobber still opens with its password.
        let data = validate_encrypted_file(&path).unwrap();
        assert!(decrypt_to_node(&data, "pw").is_ok());
    }

    #[test]
    fn create_secret_file_refuses_to_follow_a_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, b"do not touch").unwrap();
        let link = dir.path().join("vault.age");

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&victim, &link).unwrap();

            let result = create_secret_file(&link.to_string_lossy(), "pw");

            assert!(result.is_err(), "a planted symlink is not a free name");
            assert_eq!(fs::read(&victim).unwrap(), b"do not touch");
        }
        #[cfg(not(unix))]
        let _ = (victim, link);
    }

    #[test]
    fn save_tree_ignores_a_predictable_temp_symlink() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");
        let target_str = target.to_string_lossy().to_string();
        let victim = dir.path().join("victim.txt");
        fs::write(&victim, b"do not touch").unwrap();

        // The old code wrote to `.{name}.tmp`; a planted symlink there was
        // followed and truncated the link's target.
        #[cfg(unix)]
        {
            let old_temp = dir.path().join(".vault.age.tmp");
            std::os::unix::fs::symlink(&victim, &old_temp).unwrap();
        }

        save_tree(&Node::new("Root"), &target_str, b"pw").unwrap();

        assert_eq!(fs::read(&victim).unwrap(), b"do not touch");
        let data = validate_encrypted_file(&target_str).unwrap();
        assert!(decrypt_to_node(&data, "pw").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn a_created_vault_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        create_secret_file(&path.to_string_lossy(), "pw").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode,
            atomic::SECRET_FILE_MODE,
            "the vault must be owner-only"
        );
    }

    #[cfg(unix)]
    #[test]
    fn save_tree_preserves_the_existing_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        let path_str = path.to_string_lossy().to_string();

        save_tree(&Node::new("First"), &path_str, b"pw").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        save_tree(&Node::new("Second"), &path_str, b"pw").unwrap();

        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o640, "a replacement keeps the file's mode");
    }

    #[cfg(unix)]
    #[test]
    fn a_created_private_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        create_private_dir(&nested).unwrap();

        let mode = fs::metadata(&nested).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, SECRET_DIR_MODE, "the directory must be owner-only");
    }

    #[cfg(unix)]
    #[test]
    fn save_tree_does_not_inherit_a_link_targets_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        fs::write(&victim, b"victim").unwrap();
        fs::set_permissions(&victim, fs::Permissions::from_mode(0o666)).unwrap();
        let link = dir.path().join("vault.age");
        std::os::unix::fs::symlink(&victim, &link).unwrap();

        save_tree(&Node::new("Root"), &link.to_string_lossy(), b"pw").unwrap();

        // The vault replaces the link, so its mode must be owner-only, never
        // the target's 0666.
        let mode = fs::metadata(&link).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, atomic::SECRET_FILE_MODE);
        assert_eq!(fs::read(&victim).unwrap(), b"victim");
    }

    #[test]
    fn validate_encrypted_file_errors_on_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope.age");

        assert!(matches!(
            validate_encrypted_file(&missing.to_string_lossy()),
            Err(FileHandlingError::Io(_))
        ));
    }

    // ── Filesystem operation tests ──

    #[test]
    fn is_dir_empty_is_true_for_an_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_dir_empty(dir.path()).unwrap());
    }

    #[test]
    fn is_dir_empty_counts_a_hidden_entry() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"hidden").unwrap();

        assert!(
            !is_dir_empty(dir.path()).unwrap(),
            "a dot-prefixed entry still makes the directory non-empty"
        );
    }

    #[test]
    fn is_dir_empty_errors_on_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("does-not-exist");

        assert!(matches!(
            is_dir_empty(&missing),
            Err(FileHandlingError::Io(_))
        ));
    }

    #[test]
    fn test_list_dir_invalid_path() {
        let result = list_dir(Path::new("/nonexistent_dir_abc123"));
        assert!(result.is_err());
    }

    #[test]
    fn test_list_dir_non_empty() {
        let result = list_dir(Path::new("/tmp"));
        assert!(result.is_ok());
        let entries = result.unwrap();
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_list_dir_dirs_first() {
        let result = list_dir(Path::new("/tmp"));
        assert!(result.is_ok());
        let entries = result.unwrap();
        let mut seen_file = false;
        for e in &entries {
            if !e.is_dir {
                seen_file = true;
            } else if seen_file {
                panic!("directory found after file — sort order broken");
            }
        }
    }

    #[test]
    fn test_list_dir_skips_hidden_by_default() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"hidden").unwrap();
        fs::write(dir.path().join("visible.txt"), b"visible").unwrap();
        fs::create_dir(dir.path().join(".hidden_dir")).unwrap();

        let entries = list_dir(dir.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(!names.contains(&".hidden.txt"));
        assert!(!names.contains(&".hidden_dir"));
        assert!(names.contains(&"visible.txt"));
    }

    #[test]
    fn test_list_dir_including_hidden_shows_dot_entries() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".hidden.txt"), b"hidden").unwrap();
        fs::write(dir.path().join("visible.txt"), b"visible").unwrap();
        fs::create_dir(dir.path().join(".hidden_dir")).unwrap();

        let entries = list_dir_including_hidden(dir.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&".hidden.txt"));
        assert!(names.contains(&".hidden_dir"));
        assert!(names.contains(&"visible.txt"));
        // Directories still sort before files.
        let first_file = entries.iter().position(|e| !e.is_dir).unwrap();
        assert!(entries[..first_file].iter().all(|e| e.is_dir));
    }

    #[test]
    fn test_list_dir_including_hidden_marks_dot_age_file() {
        let dir = tempfile::tempdir().unwrap();
        let data = [AGE_MAGIC.as_slice(), &[0u8, 1, 2, 3]].concat();
        fs::write(dir.path().join(".secret.age"), &data).unwrap();

        let entries = list_dir_including_hidden(dir.path()).unwrap();
        let entry = entries.iter().find(|e| e.name == ".secret.age").unwrap();
        assert!(entry.is_age_file);
    }

    #[test]
    fn test_is_age_file_true() {
        let path = Path::new("/tmp/test_age_marker.age");
        let _ = fs::remove_file(path);
        let data = [AGE_MAGIC.as_slice(), &[0u8, 1, 2, 3]].concat();
        fs::write(path, &data).unwrap();
        assert!(is_age_file(path));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_is_age_file_false() {
        let path = Path::new("/tmp/test_not_age.txt");
        let _ = fs::remove_file(path);
        fs::write(path, b"hello world not age").unwrap();
        assert!(!is_age_file(path));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_is_age_file_too_small() {
        let path = Path::new("/tmp/test_tiny.txt");
        let _ = fs::remove_file(path);
        fs::write(path, b"tiny").unwrap();
        assert!(!is_age_file(path));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn test_is_age_file_nonexistent() {
        let path = Path::new("/tmp/does_not_exist_abc123.test");
        assert!(!is_age_file(path));
    }

    #[test]
    fn test_create_dir() {
        let path = Path::new("/tmp/test_create_dir_abc123");
        let _ = fs::remove_dir_all(path);
        create_dir(path).unwrap();
        assert!(path.is_dir());
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn test_create_dir_nested() {
        let path = Path::new("/tmp/a/b/c/test_nested_dir_xyz");
        let _ = fs::remove_dir_all(Path::new("/tmp/a"));
        create_dir(path).unwrap();
        assert!(path.is_dir());
        let _ = fs::remove_dir_all(Path::new("/tmp/a"));
    }

    #[test]
    fn test_delete_file_ok() {
        let path = Path::new("/tmp/__test_delete_file__.tmp");
        let _ = fs::remove_file(path);
        fs::write(path, b"test").unwrap();
        assert!(path.exists());
        delete_file(path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn test_delete_file_nonexistent() {
        let path = Path::new("/tmp/__test_delete_nonexistent__.tmp");
        let _ = fs::remove_file(path);
        assert!(delete_file(path).is_err());
    }

    #[test]
    fn test_rename_file_ok() {
        let from = Path::new("/tmp/__test_rename_from__.tmp");
        let to = Path::new("/tmp/__test_rename_to__.tmp");
        let _ = fs::remove_file(from);
        let _ = fs::remove_file(to);
        fs::write(from, b"data").unwrap();
        rename_file(from, to).unwrap();
        assert!(!from.exists());
        assert!(to.exists());
        let _ = fs::remove_file(to);
    }

    #[test]
    fn test_rename_file_nonexistent_from() {
        let from = Path::new("/tmp/__test_rename_nonexistent__.tmp");
        let to = Path::new("/tmp/__test_rename_target__.tmp");
        let _ = fs::remove_file(from);
        let _ = fs::remove_file(to);
        assert!(rename_file(from, to).is_err());
    }

    #[test]
    fn create_secret_file_writes_an_openable_vault() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        let path = path.to_string_lossy().to_string();

        let node = create_secret_file(&path, "test-passphrase").unwrap();

        assert_eq!(node.name, texts::DEFAULT_ROOT_NAME);
        let data = validate_encrypted_file(&path).unwrap();
        assert!(decrypt_to_node(&data, "test-passphrase").is_ok());
    }

    #[test]
    fn decrypt_to_node_opens_a_written_vault() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        let path = path.to_string_lossy().to_string();

        create_test_encrypted_file(&path, "test-passphrase", &Node::new("Secrets"));

        let data = validate_encrypted_file(&path).expect("validation should succeed");
        let node = decrypt_to_node(&data, "test-passphrase").expect("decryption should succeed");

        assert_eq!(node.name, "Secrets");
    }

    #[test]
    fn save_tree_rejects_a_non_utf8_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        let path = path.to_string_lossy().to_string();

        let result = save_tree(&Node::new("X"), &path, &[0xff, 0xfe]);

        assert!(matches!(
            result,
            Err(FileHandlingError::InvalidPassphraseEncoding)
        ));
        assert!(!Path::new(&path).exists());
    }

    #[test]
    fn a_failed_rename_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        // A directory at the target path makes `rename` fail.
        let target = dir.path().join("vault.age");
        fs::create_dir(&target).unwrap();

        let result = save_tree(&Node::new("X"), &target.to_string_lossy(), b"pw");

        assert!(result.is_err());
        assert!(target.is_dir(), "the directory is untouched");
        let leftovers: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temp file may survive: {leftovers:?}"
        );
    }

    #[test]
    fn validate_rejects_a_file_one_byte_over_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(file);

        assert!(matches!(
            validate_encrypted_file(&path.to_string_lossy()),
            Err(FileHandlingError::FileTooLarge { .. })
        ));
    }

    #[test]
    fn validate_accepts_a_file_exactly_at_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.age");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(AGE_MAGIC).unwrap();
        file.set_len(MAX_FILE_SIZE).unwrap();
        drop(file);

        let data = validate_encrypted_file(&path.to_string_lossy()).unwrap();
        assert_eq!(data.len() as u64, MAX_FILE_SIZE);
    }

    #[test]
    fn probe_path_writable_accepts_a_writable_directory_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        assert!(probe_path_writable(&target.to_string_lossy()).is_ok());
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            0,
            "the probe must not leave its temp file behind"
        );
    }

    #[test]
    fn probe_path_writable_rejects_a_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing").join("vault.age");

        assert!(matches!(
            probe_path_writable(&missing.to_string_lossy()),
            Err(FileHandlingError::PathNotWritable(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn list_dir_does_not_block_on_a_fifo() {
        use std::process::Command;
        use std::sync::mpsc;
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let fifo = dir.path().join("pipe");
        // Opening a FIFO for reading blocks until a writer appears; `list_dir`
        // must skip it rather than hang the event-loop thread.
        let created = Command::new("mkfifo").arg(&fifo).status();
        if !matches!(created, Ok(status) if status.success()) {
            return; // `mkfifo` is unavailable here.
        }

        let (tx, rx) = mpsc::channel();
        let scan_dir = dir.path().to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(list_dir(&scan_dir));
        });

        let entries = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("list_dir must not block on a FIFO")
            .unwrap();
        let entry = entries.iter().find(|e| e.name == "pipe").unwrap();
        assert!(!entry.is_age_file, "a FIFO must never be scanned");
    }

    #[test]
    fn read_file_and_write_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.bin");
        let path = path.to_string_lossy().to_string();

        write_file(&path, b"hello").unwrap();

        assert_eq!(read_file(&path).unwrap(), b"hello");
    }
}
