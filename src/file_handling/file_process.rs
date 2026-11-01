use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zeroize::Zeroizing;

use super::FileHandlingError;
use crate::crypt;
use crate::theming::texts;
use crate::tree::node::Node;

/// The maximum size (in bytes) of a vault or import the gateway will read.
pub(crate) const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
/// The age file-format magic header every encrypted vault begins with.
const AGE_MAGIC: &[u8; 11] = b"age-encrypt";
/// Mode (Unix) new secret-bearing files are created with: owner only.
const SECRET_FILE_MODE: u32 = 0o600;
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

/// Reads the whole of `path` into memory.
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when `path` cannot be read.
pub fn read_file(path: &str) -> Result<Vec<u8>, FileHandlingError> {
    fs::read(path).map_err(FileHandlingError::Io)
}

/// Writes `data` to `path` in a single call.
///
/// **Not atomic** and it does not restrict permissions: intended only for the
/// caller's non-secret configuration (`vault::utils`), never for a vault or an
/// export — use [`save_tree`] or
/// [`export_tree`](crate::file_handling::export_tree) for those.
///
/// # Errors
///
/// Returns [`FileHandlingError::Io`] when `path` cannot be written.
pub fn write_file(path: &str, data: &[u8]) -> Result<(), FileHandlingError> {
    fs::write(path, data).map_err(FileHandlingError::Io)
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
    let file = fs::File::open(path).map_err(FileHandlingError::Io)?;
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

/// A temporary file created exclusively under a random name beside its target.
///
/// [`create_with_mode`] opens it with `O_CREAT | O_EXCL`, so a pre-planted
/// symlink at the temp path is never followed and the write can never truncate
/// the link's target; the random suffix also means two concurrent writers never
/// collide. Dropping the guard removes the file unless it was
/// [`TempFile::persist`]ed, so a failed write leaves nothing behind.
struct TempFile {
    path: PathBuf,
    file: Option<fs::File>,
    persisted: bool,
}

impl TempFile {
    /// Creates a temp file for `target` in `target`'s own directory, with
    /// `mode` (Unix only) and a random suffix.
    ///
    /// # Errors
    ///
    /// Returns [`FileHandlingError::Io`] when no unique temp file can be
    /// created (for example when the directory is not writable).
    fn beside(target: &Path, mode: u32) -> Result<Self, FileHandlingError> {
        let dir = parent_dir(target);
        let name = target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("vault");

        for _ in 0..16 {
            let candidate = dir.join(format!(".{name}.{}.tmp", Uuid::new_v4().simple()));
            match create_with_mode(&candidate, mode) {
                Ok(file) => {
                    return Ok(Self {
                        path: candidate,
                        file: Some(file),
                        persisted: false,
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(FileHandlingError::Io(error)),
            }
        }

        Err(FileHandlingError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not create a unique temporary file",
        )))
    }

    /// Writes `data` and `fsync`s the file.
    fn write_all_and_sync(&mut self, data: &[u8]) -> Result<(), FileHandlingError> {
        let file = self
            .file
            .as_mut()
            .expect("the temp file is open until it is persisted");
        file.write_all(data).map_err(FileHandlingError::Io)?;
        file.sync_all().map_err(FileHandlingError::Io)
    }

    /// Atomically renames the temp file over `target`, then best-effort
    /// `fsync`s the containing directory so the rename itself is durable.
    fn persist(mut self, target: &Path) -> Result<(), FileHandlingError> {
        // Close the handle first so the rename can replace an open target.
        self.file = None;
        fs::rename(&self.path, target).map_err(FileHandlingError::Io)?;
        self.persisted = true;
        sync_parent_dir(target);
        Ok(())
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if !self.persisted {
            self.file = None;
            let _ = fs::remove_file(&self.path); // best-effort cleanup
        }
    }
}

/// Opens `path` for writing, creating it exclusively with `mode` (Unix only).
fn create_with_mode(path: &Path, mode: u32) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    // `mode` is only meaningful on Unix; this keeps it "used" elsewhere.
    let _ = mode;
    options.open(path)
}

/// The directory holding `path`, or `.` for a bare file name.
fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|dir| !dir.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

/// Best-effort `fsync` of `path`'s parent directory so a rename into it is
/// durable after a crash. Not every platform allows this, so it is ignored.
fn sync_parent_dir(path: &Path) {
    if let Ok(handle) = fs::File::open(parent_dir(path)) {
        let _ = handle.sync_all();
    }
}

/// Copies `target`'s permission bits onto a freshly created temp file, so
/// replacing an existing regular file preserves the mode the user gave it.
///
/// Uses `symlink_metadata`: a mode is never inherited from a symlink's target,
/// only from an ordinary existing file.
fn preserve_target_mode(target: &Path, file: &fs::File) -> Result<(), FileHandlingError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::symlink_metadata(target)
            && meta.is_file()
        {
            let mode = meta.permissions().mode();
            file.set_permissions(fs::Permissions::from_mode(mode))
                .map_err(FileHandlingError::Io)?;
        }
    }
    #[cfg(not(unix))]
    let _ = (target, file);
    Ok(())
}

/// Writes `data` to `target` atomically, preserving the mode of an existing
/// regular target (used for ciphertext, where the user may have set a mode).
pub(crate) fn write_atomic(target: &Path, data: &[u8]) -> Result<(), FileHandlingError> {
    let mut temp = TempFile::beside(target, SECRET_FILE_MODE)?;
    preserve_target_mode(target, temp.file.as_ref().expect("temp file is open"))?;
    temp.write_all_and_sync(data)?;
    temp.persist(target)
}

/// Writes plaintext `data` to `target` atomically at owner-only permissions,
/// never inheriting a more permissive mode from an existing file — the output
/// holds decrypted secrets (an export) and must not be group/world readable.
pub(crate) fn write_atomic_private(target: &Path, data: &[u8]) -> Result<(), FileHandlingError> {
    let mut temp = TempFile::beside(target, SECRET_FILE_MODE)?;
    temp.write_all_and_sync(data)?;
    temp.persist(target)
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

    write_atomic(Path::new(path), &encrypted)
}

/// Creates a new encrypted secret file at `path` with a "Secrets" root node.
///
/// The target is claimed with `O_EXCL`, so an existing file (or a symlink) is
/// never overwritten and no pre-existing vault can be destroyed. The content is
/// then written atomically (temp + `fsync` + rename); only a failure *after*
/// the claim but before the rename — a hard kill in that window — can leave a
/// zero-byte placeholder at `path`, which the caller can remove and retry.
/// Closing that window needs a no-clobber rename, tracked as a future
/// hardening task. On any error the claimed name is released again.
///
/// # Errors
///
/// Returns [`FileHandlingError::PathNotWritable`] when `path` cannot be
/// created (for example a missing parent directory), and
/// [`FileHandlingError::Io`] with [`std::io::ErrorKind::AlreadyExists`] when
/// it already exists. Encryption and serialisation failures surface as
/// [`FileHandlingError::Crypt`] / [`FileHandlingError::Serde`].
pub fn create_secret_file(path: &str, passphrase: &str) -> Result<Node, FileHandlingError> {
    let target = Path::new(path);

    // Claim the name exclusively: `rename` at the end replaces this
    // placeholder, and `O_EXCL` refuses to follow a symlink at `path`.
    let placeholder = create_with_mode(target, SECRET_FILE_MODE).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            FileHandlingError::Io(error)
        } else {
            FileHandlingError::PathNotWritable(path.to_string())
        }
    })?;
    drop(placeholder);

    let node = Node::new(texts::DEFAULT_ROOT_NAME);
    let outcome = encrypt_node(&node, passphrase).and_then(|data| write_atomic(target, &data));
    if outcome.is_err() {
        // Do not leave a zero-byte file claiming the name.
        let _ = fs::remove_file(target); // best-effort release of the name
    }
    outcome?;

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
        let is_age_file = if is_dir {
            false
        } else {
            path.is_file() && scan_age_header(&path)
        };

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
    let mut file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut header = [0u8; 11];
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
    TempFile::beside(Path::new(path), SECRET_FILE_MODE)
        .map(|_| ())
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
        assert!(result.is_err());
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
        assert_eq!(mode, SECRET_FILE_MODE, "the vault must be owner-only");
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

    #[test]
    fn a_temp_file_is_removed_when_it_is_not_persisted() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("vault.age");

        let temp = TempFile::beside(&target, SECRET_FILE_MODE).unwrap();
        let temp_path = temp.path.clone();
        assert!(temp_path.exists());

        drop(temp);

        assert!(
            !temp_path.exists(),
            "an unpersisted temp must be cleaned up"
        );
        assert!(!target.exists(), "the target must not have been created");
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
        assert_eq!(mode, SECRET_FILE_MODE);
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
    fn test_create_file_timing() {
        let path = format!("/tmp/test_create_time_{}.age", uuid::Uuid::new_v4());
        let passphrase = "test-passphrase";

        let start = std::time::Instant::now();
        let result = create_secret_file(&path, passphrase);
        let elapsed = start.elapsed();

        assert!(result.is_ok());
        assert!(Path::new(&path).exists());

        println!(
            "create_secret_file took {:?} ({}.{:03} ms)",
            elapsed,
            elapsed.as_millis(),
            elapsed.subsec_millis(),
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_open_file_timing() {
        let path = format!("/tmp/test_open_time_{}.age", uuid::Uuid::new_v4());
        let passphrase = "test-passphrase";

        let root = Node::new("Secrets");
        create_test_encrypted_file(&path, passphrase, &root);

        let data = validate_encrypted_file(&path).expect("validation should succeed");

        let start = std::time::Instant::now();
        let node = decrypt_to_node(&data, passphrase);
        let elapsed = start.elapsed();

        assert!(node.is_ok());
        assert_eq!(node.unwrap().name, "Secrets");

        println!(
            "decrypt_to_node took {:?} ({}.{:03} ms)",
            elapsed,
            elapsed.as_millis(),
            elapsed.subsec_millis(),
        );

        let _ = fs::remove_file(&path);
    }
}
