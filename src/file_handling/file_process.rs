use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::crypt;
use crate::file_handling::FileHandlingError;
use crate::theming::texts;
use crate::tree::node::Node;

const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024;
const AGE_MAGIC: &[u8; 11] = b"age-encrypt";

/// A single entry in a directory listing.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_age_file: bool,
}

// ── JSON helpers ──

pub fn node_to_json(node: &Node) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(node)
}

pub fn json_to_node(json: &str) -> Result<Node, serde_json::Error> {
    serde_json::from_str(json)
}

// ── Raw I/O ──

pub fn read_file(path: &str) -> Result<Vec<u8>, FileHandlingError> {
    fs::read(path).map_err(FileHandlingError::Io)
}

pub fn write_file(path: &str, data: &[u8]) -> Result<(), FileHandlingError> {
    fs::write(path, data).map_err(FileHandlingError::Io)
}

// ── Encrypted file operations ──

/// Validates and reads an age-encrypted file.  Returns the full bytes.
pub fn validate_encrypted_file(path: &str) -> Result<Vec<u8>, FileHandlingError> {
    let meta = fs::metadata(path).map_err(FileHandlingError::Io)?;

    if meta.len() > MAX_FILE_SIZE {
        return Err(FileHandlingError::FileTooLarge {
            size: meta.len(),
            max: MAX_FILE_SIZE,
        });
    }

    if meta.len() < 11 {
        return Err(FileHandlingError::FileTooSmall);
    }

    let mut file = fs::File::open(path).map_err(FileHandlingError::Io)?;

    let mut header = [0u8; 11];
    file.read_exact(&mut header)?;

    if &header != AGE_MAGIC {
        return Err(FileHandlingError::NotAnAgeFile);
    }

    let mut data = header.to_vec();
    file.read_to_end(&mut data)?;

    Ok(data)
}

/// Decrypts age-encrypted bytes with a passphrase and deserialises into a Node.
pub fn decrypt_to_node(data: &[u8], passphrase: &str) -> Result<Node, FileHandlingError> {
    let json = crypt::decrypt(data, passphrase).map_err(FileHandlingError::Crypt)?;
    json_to_node(&json).map_err(FileHandlingError::Serde)
}

/// Saves a Node tree to an age-encrypted file with an atomic write.
///
/// The new ciphertext is written to a temporary file in the same directory,
/// flushed and `sync`ed, then atomically renamed over the target path so a
/// crash mid-write never leaves a truncated/corrupt vault. The previous file
/// is temporarily preserved as `{path}.bak` during the write and removed
/// again once the save succeeds; it only survives if the process is
/// interrupted mid-save.
pub fn save_tree(root: &Node, path: &str, passphrase: &[u8]) -> Result<(), FileHandlingError> {
    let passphrase_str = std::str::from_utf8(passphrase)
        .map_err(|_| FileHandlingError::InvalidPassphraseEncoding)?;

    let json = node_to_json(root)?;
    let encrypted = crypt::encrypt(&json, passphrase_str).map_err(FileHandlingError::Crypt)?;

    let target = Path::new(path);
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let backup_path = format!("{}.bak", path);

    // Write to a unique temp file in the same directory, then atomically
    // rename over the target so a failed write never corrupts the vault.
    let tmp_path = parent.join(format!(
        ".{}.tmp",
        target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "vault".to_string())
    ));
    write_and_sync(&tmp_path, &encrypted)?;

    // Preserve the current file as a backup before replacing it.
    if target.exists() {
        fs::copy(target, &backup_path).map_err(FileHandlingError::Io)?;
    }

    fs::rename(&tmp_path, target).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        FileHandlingError::Io(e)
    })?;

    let _ = fs::remove_file(&backup_path);

    Ok(())
}

/// Writes `data` to `path`, flushing and syncing to disk before returning.
fn write_and_sync(path: &Path, data: &[u8]) -> Result<(), FileHandlingError> {
    use std::io::Write;
    let mut file = fs::File::create(path).map_err(FileHandlingError::Io)?;
    file.write_all(data).map_err(FileHandlingError::Io)?;
    file.sync_all().map_err(FileHandlingError::Io)?;
    Ok(())
}

/// Creates a new encrypted secret file at the given path with a "Secrets" root node.
pub fn create_secret_file(path: &str, passphrase: &str) -> Result<Node, FileHandlingError> {
    let probe_path = format!("{}.tmp", path);
    fs::write(&probe_path, b"probe")
        .map_err(|_| FileHandlingError::PathNotWritable(path.to_string()))?;
    let _ = fs::remove_file(&probe_path);

    let node = Node::new(texts::DEFAULT_ROOT_NAME);
    let json = node_to_json(&node)?;
    let encrypted = crypt::encrypt(&json, passphrase).map_err(FileHandlingError::Crypt)?;
    fs::write(path, &encrypted)?;
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
        let name = entry.file_name().into_string().unwrap_or_default();
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

    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir)
        } else {
            a.name.to_lowercase().cmp(&b.name.to_lowercase())
        }
    });

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

pub fn create_dir(path: &Path) -> Result<(), FileHandlingError> {
    fs::create_dir_all(path)?;
    Ok(())
}

pub fn delete_file(path: &Path) -> Result<(), FileHandlingError> {
    fs::remove_file(path)?;
    Ok(())
}

pub fn rename_file(from: &Path, to: &Path) -> Result<(), FileHandlingError> {
    fs::rename(from, to)?;
    Ok(())
}

/// Checks whether a path is writable by creating and removing a temp file.
pub fn probe_path_writable(path: &str) -> Result<(), FileHandlingError> {
    let probe = format!("{}.tmp", path);
    fs::write(&probe, b"").map_err(|_| FileHandlingError::PathNotWritable(path.to_string()))?;
    let _ = fs::remove_file(&probe);
    Ok(())
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypt::CryptError;
    use crate::file_handling::FileHandlingError;
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
    fn test_save_tree_backup_and_write() {
        let path = format!("/tmp/test_save_{}.age", uuid::Uuid::new_v4());
        let backup_path = format!("{}.bak", path);
        let passphrase = b"save-test-pass";
        let passphrase_str = "save-test-pass";

        let initial = Node::new("Initial");
        create_test_encrypted_file(&path, passphrase_str, &initial);

        let mutated = Node::with_children("Updated Root", vec![Node::new("New Child")]);
        save_tree(&mutated, &path, passphrase).unwrap();

        // After a successful save the temporary backup is removed again.
        assert!(fs::metadata(&backup_path).is_err());

        let data = fs::read(&path).unwrap();
        let decoded = decrypt_to_node(&data, passphrase_str).unwrap();
        assert_eq!(decoded.name, "Updated Root");
        assert_eq!(decoded.children.len(), 1);
        assert_eq!(decoded.children[0].name, "New Child");

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&backup_path);
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
        let _ = fs::remove_file(format!("{}.bak", path));
    }

    #[test]
    fn test_save_tree_empty_backup() {
        let path = format!("/tmp/test_no_backup_{}.age", uuid::Uuid::new_v4());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}.bak", path));

        let node = Node::new("New");
        save_tree(&node, &path, b"pass").unwrap();

        assert!(fs::metadata(&path).is_ok());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}.bak", path));
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

    // ── Filesystem operation tests ──

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
