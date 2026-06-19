use std::fs;
use std::io::Read;
use std::path::Path;

use super::state::FileEntry;

const AGE_MAGIC: &[u8; 11] = b"age-encrypt";

/// Lists all entries in `path`, sorted with directories first then
/// alphabetically (case-insensitive).  Each regular file is scanned
/// for the age-encryption magic header.
pub fn list_dir(path: &Path) -> Result<Vec<FileEntry>, String> {
    let mut entries: Vec<FileEntry> = Vec::new();

    let read_dir = fs::read_dir(path).map_err(|e| format!("Cannot read directory: {e}"))?;

    for entry in read_dir {
        let entry = entry.map_err(|e| format!("Cannot read entry: {e}"))?;
        let name = entry
            .file_name()
            .into_string()
            .unwrap_or_default();
        // Skip hidden files (starting with '.')
        if name.starts_with('.') {
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

    // Sort: dirs first, then files, alphabetical within each group
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            b.is_dir.cmp(&a.is_dir) // dirs first
        } else {
            let a_lower = a.name.to_lowercase();
            let b_lower = b.name.to_lowercase();
            a_lower.cmp(&b_lower)
        }
    });

    Ok(entries)
}

/// Returns `true` if the file starts with the age-encryption magic header.
#[allow(dead_code)]
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

/// Creates a directory and all parent directories as needed.
pub fn create_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("Cannot create directory: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // /tmp should have at least some entries
        assert!(!entries.is_empty());
    }

    #[test]
    fn test_list_dir_dirs_first() {
        let result = list_dir(Path::new("/tmp"));
        assert!(result.is_ok());
        let entries = result.unwrap();
        // dirs should all come before files
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
}
