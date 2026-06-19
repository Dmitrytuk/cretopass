use std::fs;
use std::io::Read;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::crypt;
use crate::file_handling;
use crate::tree::node::Node;

const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB
const SETTINGS_FILE_NAME: &str = "settings.pwd";

/// Persistent app settings stored in `~/.config/pwd-keeper/settings.pwd`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// Recently opened secret files (most recent first).
    pub recent_files: Vec<String>,
    /// The last file the user opened or created.
    pub last_opened_file: Option<String>,
    /// If `true`, window title shows the full path; otherwise just the filename.
    pub display_full_path: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            recent_files: Vec::new(),
            last_opened_file: None,
            display_full_path: true,
        }
    }
}

/// Returns the platform-specific config directory for pwd-keeper.
///
/// - Linux: `$XDG_CONFIG_HOME/pwd-keeper/` or `~/.config/pwd-keeper/`
/// - macOS: `~/Library/Application Support/pwd-keeper/`
/// - Windows: `%APPDATA%/pwd-keeper/`
/// - Fallback: `./.pwd-keeper/`
fn config_dir() -> PathBuf {
    if cfg!(target_os = "linux") {
        if let Ok(val) = std::env::var("XDG_CONFIG_HOME") {
            let mut p = PathBuf::from(val);
            p.push("pwd-keeper");
            return p;
        }
        if let Ok(val) = std::env::var("HOME") {
            let mut p = PathBuf::from(val);
            p.push(".config/pwd-keeper");
            return p;
        }
    } else if cfg!(target_os = "macos") {
        if let Ok(val) = std::env::var("HOME") {
            let mut p = PathBuf::from(val);
            p.push("Library/Application Support/pwd-keeper");
            return p;
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(val) = std::env::var("APPDATA") {
            let mut p = PathBuf::from(val);
            p.push("pwd-keeper");
            return p;
        }
    }
    let mut p = PathBuf::from(".");
    p.push(".pwd-keeper");
    p
}

/// Validates that the file at `path` exists, is ≤ 50 MB, and starts with the
/// age-encryption magic header.  Returns the full file contents on success.
pub fn validate_encrypted_file(path: &str) -> Result<Vec<u8>, String> {
    let meta = fs::metadata(path).map_err(|e| format!("Cannot access file: {e}"))?;

    if meta.len() > MAX_FILE_SIZE {
        return Err(format!(
            "File too large: {} MB (max 50 MB)",
            meta.len() / (1024 * 1024)
        ));
    }

    if meta.len() < 11 {
        return Err("File is too small to be an age-encrypted file".to_string());
    }

    let mut file = fs::File::open(path).map_err(|e| format!("Cannot open file: {e}"))?;

    let mut header = [0u8; 11];
    file.read_exact(&mut header)
        .map_err(|_| "Cannot read file header".to_string())?;

    if &header != b"age-encrypt" {
        return Err(
            "File is not an age-encrypted file (missing age-encrypt magic header)".to_string(),
        );
    }

    let mut data = header.to_vec();
    file.read_to_end(&mut data)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    Ok(data)
}

/// Decrypts age-encrypted bytes with a passphrase and deserialises the
/// result into a [`Node`].
pub fn decrypt_to_node(data: &[u8], passphrase: &str) -> Result<Node, String> {
    let json = crypt::decrypt(data, passphrase)
        .map_err(|e| format!("Decryption failed: {e}"))?;

    file_handling::json_to_node(&json).map_err(|e| format!("Failed to parse file data: {e}"))
}

/// Saves the tree to the given file path.
///
/// 1. Creates a backup of the current file at `{path}.bak`.
/// 2. Serialises `root` to JSON via `file_handling`.
/// 3. Encrypts the JSON with the passphrase via `crypt`.
/// 4. Writes the encrypted bytes to `path`.
pub fn save_tree(root: &Node, path: &str, passphrase: &[u8]) -> Result<(), String> {
    let passphrase_str =
        std::str::from_utf8(passphrase).map_err(|_| "Invalid passphrase encoding".to_string())?;

    let json = file_handling::node_to_json(root)
        .map_err(|e| format!("Failed to serialise tree: {e}"))?;

    let encrypted =
        crypt::encrypt(&json, passphrase_str).map_err(|e| format!("Encryption failed: {e}"))?;

    // Backup existing file
    let backup_path = format!("{}.bak", path);
    let _ = fs::copy(path, &backup_path);

    // Write new file
    fs::write(path, &encrypted).map_err(|e| format!("Failed to write file: {e}"))?;

    // Remove backup on success
    let _ = fs::remove_file(&backup_path);

    Ok(())
}

/// Saves app settings to `~/.config/pwd-keeper/settings.pwd` as binary.
pub fn save_settings(settings: &AppSettings) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Cannot create config directory: {e}"))?;

    let path = dir.join(SETTINGS_FILE_NAME);
    let bytes =
        postcard::to_allocvec(settings).map_err(|e| format!("Failed to serialise settings: {e}"))?;

    fs::write(&path, &bytes).map_err(|e| format!("Failed to write settings: {e}"))?;

    Ok(())
}

/// Loads app settings from the config directory.
///
/// Returns default settings if the file does not exist.
pub fn load_settings() -> Result<AppSettings, String> {
    let path = config_dir().join(SETTINGS_FILE_NAME);

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppSettings::default());
        }
        Err(e) => return Err(format!("Failed to read settings: {e}")),
    };

    let settings: AppSettings = postcard::from_bytes(&bytes)
        .map_err(|e| format!("Failed to parse settings: {e}"))?;

    Ok(settings)
}

/// Attempts to open an OS-native file-selection dialog.
///
/// Tries, in order:
/// - Linux: `zenity --file-selection`
/// - Linux: `kdialog --getopenfilename`
/// - macOS: `osascript -e 'POSIX path of (choose file)'`
/// - Windows: `powershell -Command ...`
///
/// Returns `None` if no dialog tool is available or the user cancels.
pub fn open_file_dialog() -> Option<String> {
    if cfg!(target_os = "linux") {
        let result = std::process::Command::new("zenity")
            .args(["--file-selection"])
            .output();
        if let Ok(output) = result {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }

        let result = std::process::Command::new("kdialog")
            .args(["--getopenfilename", "."])
            .output();
        if let Ok(output) = result {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    } else if cfg!(target_os = "macos") {
        let result = std::process::Command::new("osascript")
            .args(["-e", "POSIX path of (choose file)"])
            .output();
        if let Ok(output) = result {
            if output.status.success() {
                let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !path.is_empty() {
                    return Some(path);
                }
            }
        }
    } else if cfg!(target_os = "windows") {
        let result = std::process::Command::new("powershell")
            .args([
                "-Command",
                "Add-Type -AssemblyName System.Windows.Forms; \
                 $f = New-Object System.Windows.Forms.OpenFileDialog; \
                 $f.ShowDialog() | Out-Null; \
                 Write-Host $f.FileName",
            ])
            .output();
        if let Ok(output) = result {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(path);
            }
        }
    }

    None
}

/// Inserts or moves `path` to the front of the recent-files list and saves.
/// Duplicates are removed first.
pub fn push_recent_file(settings: &mut AppSettings, path: &str) -> Result<(), String> {
    settings.recent_files.retain(|f| f != path);
    settings.recent_files.insert(0, path.to_string());
    settings.last_opened_file = Some(path.to_string());
    save_settings(settings)
}

/// Creates a new encrypted secret file at the given path.
///
/// 1. Writes a temporary file to verify the path is writable.
/// 2. Creates a [`Node`] with a single root named `"Secrets"`.
/// 3. Serialises to JSON, encrypts with the passphrase, and writes.
/// 4. Removes the temporary file on success.
///
/// Returns the created [`Node`] on success.
pub fn create_secret_file(path: &str, passphrase: &str) -> Result<Node, String> {
    // Probe writability by creating and removing a temp file
    let probe_path = format!("{}.tmp", path);
    fs::write(&probe_path, b"probe")
        .map_err(|e| format!("Cannot write to path: {e}"))?;
    let _ = fs::remove_file(&probe_path);

    let node = Node::new("Secrets", "");
    let json =
        file_handling::node_to_json(&node).map_err(|e| format!("Failed to serialise: {e}"))?;
    let encrypted =
        crate::crypt::encrypt(&json, passphrase).map_err(|e| format!("Encryption failed: {e}"))?;
    fs::write(path, &encrypted).map_err(|e| format!("Failed to write file: {e}"))?;
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypt;
    use crate::tree::node::{ExtraField, FieldType};

    /// Helper: create a valid age-encrypted file containing a JSON node.
    fn create_test_encrypted_file(path: &str, passphrase: &str, node: &Node) {
        let json = file_handling::node_to_json(node).unwrap();
        let encrypted = crypt::encrypt(&json, passphrase).unwrap();
        fs::write(path, &encrypted).unwrap();
    }

    #[test]
    fn test_validate_rejects_large_file() {
        let path = format!("/tmp/test_large_{}.bin", uuid::Uuid::new_v4());
        // Create a sparse file that reports > 50 MB
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(file);

        let result = validate_encrypted_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too large"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_rejects_non_age_file() {
        let path = format!("/tmp/test_not_age_{}.txt", uuid::Uuid::new_v4());
        fs::write(&path, b"this is not an age file").unwrap();

        let result = validate_encrypted_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not an age-encrypted"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_validate_accepts_valid_file() {
        let path = format!("/tmp/test_valid_{}.age", uuid::Uuid::new_v4());
        let node = Node::new("Test", "valid file test");
        create_test_encrypted_file(&path, "test-pass", &node);

        let result = validate_encrypted_file(&path);
        assert!(result.is_ok());
        assert!(result.unwrap().starts_with(b"age-encrypt"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_decrypt_to_node_roundtrip() {
        let path = format!("/tmp/test_decrypt_{}.age", uuid::Uuid::new_v4());
        let original = Node::with_children(
            "Root",
            "root desc",
            vec![Node::new("Child1", "leaf")],
        );
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
        let node = Node::new("Test", "wrong pass test");
        create_test_encrypted_file(&path, "correct-pass", &node);

        let data = fs::read(&path).unwrap();
        let result = decrypt_to_node(&data, "wrong-pass");
        assert!(result.is_err());

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_save_tree_backup_and_write() {
        let path = format!("/tmp/test_save_{}.age", uuid::Uuid::new_v4());
        let backup_path = format!("{}.bak", path);
        let passphrase = b"save-test-pass";
        let passphrase_str = "save-test-pass";

        // Create an initial file
        let initial = Node::new("Initial", "before save");
        create_test_encrypted_file(&path, passphrase_str, &initial);

        // Mutate and save
        let mutated = Node::with_children(
            "Updated Root",
            "after mutation",
            vec![Node::new("New Child", "added")],
        );
        save_tree(&mutated, &path, passphrase).unwrap();

        // Verify backup was cleaned up
        assert!(fs::metadata(&backup_path).is_err());

        // Verify new file can be read
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

        let mut node = Node::new("Entry", "has extra fields");
        node.extra_fields = vec![
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

        // Re-open and verify
        let data = fs::read(&path).unwrap();
        let decoded = decrypt_to_node(&data, passphrase_str).unwrap();
        assert_eq!(decoded.extra_fields.len(), 2);
        assert_eq!(decoded.extra_fields[0].value, "admin");

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&format!("{}.bak", path));
    }

    #[test]
    fn test_save_tree_empty_backup() {
        // Save when there is no existing file — backup step should no-op
        let path = format!("/tmp/test_no_backup_{}.age", uuid::Uuid::new_v4());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&format!("{}.bak", path));

        let node = Node::new("New", "first save");
        save_tree(&node, &path, b"pass").unwrap();

        assert!(fs::metadata(&path).is_ok());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&format!("{}.bak", path));
    }

    fn clean_settings_file() {
        let path = config_dir().join(SETTINGS_FILE_NAME);
        if path.exists() {
            let _ = fs::remove_file(&path);
        }
    }

    #[test]
    fn test_settings_save_load() {
        clean_settings_file();

        let settings = AppSettings {
            recent_files: vec![
                "/home/user/vault.age".to_string(),
                "/home/user/backup.age".to_string(),
            ],
            last_opened_file: Some("/home/user/vault.age".to_string()),
            display_full_path: false,
        };
        save_settings(&settings).unwrap();
        let loaded = load_settings().unwrap();

        assert_eq!(loaded.recent_files.len(), 2);
        assert_eq!(loaded.recent_files[0], "/home/user/vault.age");
        assert_eq!(loaded.recent_files[1], "/home/user/backup.age");
        assert_eq!(loaded.last_opened_file, Some("/home/user/vault.age".to_string()));
        assert!(!loaded.display_full_path);

        clean_settings_file();
    }

    #[test]
    fn test_settings_default_display_full_path() {
        let settings = AppSettings::default();
        assert!(settings.display_full_path);
        assert!(settings.recent_files.is_empty());
        assert!(settings.last_opened_file.is_none());
    }

    #[test]
    fn test_settings_save_empty() {
        clean_settings_file();

        let settings = AppSettings::default();
        save_settings(&settings).unwrap();
        let loaded = load_settings().unwrap();
        assert!(loaded.recent_files.is_empty());
        assert!(loaded.last_opened_file.is_none());
        assert!(loaded.display_full_path);

        clean_settings_file();
    }

    #[test]
    fn test_settings_no_file_returns_defaults() {
        clean_settings_file();
        let loaded = load_settings().unwrap();
        assert!(loaded.recent_files.is_empty());
        assert!(loaded.display_full_path);
    }

    #[test]
    fn test_push_recent_file_dedup() {
        clean_settings_file();

        let mut settings = AppSettings {
            recent_files: vec![
                "/alpha.age".to_string(),
                "/beta.age".to_string(),
                "/gamma.age".to_string(),
            ],
            last_opened_file: None,
            display_full_path: true,
        };
        push_recent_file(&mut settings, "/beta.age").unwrap();

        assert_eq!(settings.recent_files.len(), 3);
        assert_eq!(settings.recent_files[0], "/beta.age");  // moved to front
        assert_eq!(settings.recent_files[1], "/alpha.age");
        assert_eq!(settings.recent_files[2], "/gamma.age");
        assert_eq!(
            settings.last_opened_file,
            Some("/beta.age".to_string())
        );

        clean_settings_file();
    }

    #[test]
    fn test_push_recent_file_sets_last_opened() {
        clean_settings_file();

        let mut settings = AppSettings::default();
        push_recent_file(&mut settings, "/new.age").unwrap();

        assert_eq!(settings.recent_files.len(), 1);
        assert_eq!(settings.recent_files[0], "/new.age");
        assert_eq!(settings.last_opened_file, Some("/new.age".to_string()));

        clean_settings_file();
    }

    #[test]
    fn test_settings_roundtrip_display_full_path_false() {
        clean_settings_file();

        let mut settings = AppSettings::default();
        settings.display_full_path = false;
        settings.recent_files.push("/a.age".to_string());
        save_settings(&settings).unwrap();

        let loaded = load_settings().unwrap();
        assert!(!loaded.display_full_path);

        clean_settings_file();
    }

    #[test]
    fn test_validate_rejects_too_small() {
        let path = format!("/tmp/test_tiny_{}.bin", uuid::Uuid::new_v4());
        fs::write(&path, b"tiny").unwrap();

        let result = validate_encrypted_file(&path);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too small"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_create_secret_file_ok() {
        let path = format!("/tmp/test_create_ok_{}.age", uuid::Uuid::new_v4());
        let _ = fs::remove_file(&path);

        let node = create_secret_file(&path, "test-pass").unwrap();

        assert_eq!(node.name, "Secrets");
        assert!(node.children.is_empty());

        // File should exist and contain valid encrypted data
        assert!(fs::metadata(&path).is_ok());
        let data = fs::read(&path).unwrap();
        assert!(data.starts_with(b"age-encrypt"));

        // Decrypt and verify node
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
}
