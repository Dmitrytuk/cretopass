//! Exporting a Node tree to plain text — JSON, XML, or CSV.
//!
//! The destination is named by the user in the File Browser. [`suggest_export_name`] and
//! [`output_path`] are the single source of truth for the file name, and
//! [`export_tree`] refuses to overwrite a secret file (see
//! [ADR 0022](../../docs/adr/0022-user-chosen-export-destination.md)).

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

use super::format::{Format, encode_path_segment};
use super::{FileHandlingError, is_age_file, node_to_json};
use crate::tree::node::Node;

/// The formats an exported document may use.
pub type ExportFormat = Format;

/// The name an export is suggested with: the stem of the secret file being
/// exported, the Node's name when a single Node is exported, and the format's
/// extension.
///
/// Every character in the stem and in `node_name` that is not alphanumeric or
/// `-`/`_` becomes `_`, so the suggestion is always a usable file name.
pub fn suggest_export_name(
    base_path: Option<&Path>,
    node_name: Option<&str>,
    format: ExportFormat,
) -> String {
    let stem = sanitize(
        base_path
            .and_then(Path::file_stem)
            .and_then(OsStr::to_str)
            .unwrap_or("export"),
    );
    let name = match node_name {
        Some(node_name) => format!("{stem}_{}", sanitize(node_name)),
        None => stem,
    };
    format!("{name}.{}", format.extension())
}

/// Replaces every character that may not appear in a generated file name,
/// shared by the export and share flows.
pub(crate) fn sanitize(node_name: &str) -> String {
    node_name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// The file `format` writes for `path`: the same directory and stem, with the
/// format's extension. Applying it to its own result changes nothing.
pub fn output_path(path: &Path, format: ExportFormat) -> PathBuf {
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("export");
    let name = format!("{stem}.{}", format.extension());
    match path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        Some(dir) => dir.join(name),
        None => PathBuf::from(name),
    }
}

/// Writes `node` to `file_path` as `format`, and returns the path written.
///
/// The extension of `file_path` is replaced by the format's (see
/// [`output_path`]).
///
/// # Errors
///
/// Returns [`FileHandlingError::ExportSerialization`] when `node` cannot be
/// serialised, or [`FileHandlingError::ExportOverwritesSecret`] when the
/// destination is a secret file — an export is plain text, so writing it over
/// an encrypted file would destroy the vault. Returns
/// [`FileHandlingError::Io`] when the write fails.
pub fn export_tree(
    node: &Node,
    file_path: &str,
    format: ExportFormat,
) -> Result<String, FileHandlingError> {
    let out_path = output_path(Path::new(file_path), format);
    if is_age_file(&out_path) {
        return Err(FileHandlingError::ExportOverwritesSecret {
            path: out_path.display().to_string(),
        });
    }

    let data = match format {
        ExportFormat::Json => node_to_json(node).map_err(FileHandlingError::ExportSerialization)?,
        ExportFormat::Xml => node_to_xml(node),
        ExportFormat::Csv => node_to_csv(node),
    };

    // Write the plaintext export atomically, at owner-only permissions: it
    // holds decrypted secrets, so it must not be world-readable.
    super::atomic::write_atomic_private(&out_path, data.as_bytes())?;

    Ok(out_path.to_string_lossy().to_string())
}

fn node_to_xml(node: &Node) -> Zeroizing<String> {
    let mut buf = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    write_xml_node(&mut buf, node, 0);
    Zeroizing::new(buf)
}

fn write_xml_node(buf: &mut String, node: &Node, depth: usize) {
    let indent = "  ".repeat(depth);
    let name_esc = escape_xml(&node.name);

    buf.push_str(&format!("{indent}<node name=\"{name_esc}\">\n"));

    buf.push_str(&format!("{}  <extra_fields>\n", indent));
    for ef in &node.extra_fields {
        let ft = escape_xml(&ef.field_type.wire_label());
        let val = escape_xml(&ef.value);
        buf.push_str(&format!(
            "{}    <field type=\"{ft}\" value=\"{val}\"/>\n",
            indent
        ));
    }
    buf.push_str(&format!("{}  </extra_fields>\n", indent));

    if !node.children.is_empty() {
        buf.push_str(&format!("{}  <children>\n", indent));
        for child in &node.children {
            write_xml_node(buf, child, depth + 2);
        }
        buf.push_str(&format!("{}  </children>\n", indent));
    }

    buf.push_str(&format!("{indent}</node>\n"));
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn node_to_csv(node: &Node) -> Zeroizing<String> {
    let mut buf = String::from("path,name,field_type,field_value\n");
    write_csv_rows(&mut buf, node, "", &encode_path_segment(&node.name));
    Zeroizing::new(buf)
}

/// Writes `node` and its subtree, one row per extra field.
///
/// `segment` is `node`'s already-encoded `path` segment. Children get their own
/// segment, with a `#<n>` occurrence suffix when two siblings share a name, so a
/// duplicate sibling is never folded into its twin on import (see
/// [`encode_path_segment`] and [`decode_path_segment`]).
fn write_csv_rows(buf: &mut String, node: &Node, parent_path: &str, segment: &str) {
    let current_path = if parent_path.is_empty() {
        segment.to_string()
    } else {
        format!("{parent_path} > {segment}")
    };

    // The name column carries the real name, so the reader need not decode the
    // path's last segment for display; the path is only the structural key.
    let path = csv_escape(&current_path);
    let name = csv_escape(&node.name);

    if node.extra_fields.is_empty() {
        buf.push_str(&format!("{path},{name},,\n"));
    } else {
        for ef in &node.extra_fields {
            let ft = csv_escape(&ef.field_type.wire_label());
            let val = csv_escape(&ef.value);
            buf.push_str(&format!("{path},{name},{ft},{val}\n"));
        }
    }

    let mut used: HashMap<&str, usize> = HashMap::new();
    for child in &node.children {
        let count = used.entry(child.name.as_str()).or_insert(0);
        *count += 1;
        let encoded = encode_path_segment(&child.name);
        let child_segment = if *count == 1 {
            encoded
        } else {
            format!("{encoded}#{}", *count)
        };
        write_csv_rows(buf, child, &current_path, &child_segment);
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::{ExtraField, FieldType};

    fn sample_node() -> Node {
        let mut child = Node::new("Login");
        child.extra_fields = vec![
            ExtraField::description("My email login"),
            ExtraField {
                field_type: FieldType::Login,
                value: "user@example.com".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "s3cret".to_string(),
            },
        ];
        let mut root = Node::with_children("Secrets", vec![child]);
        root.extra_fields = vec![
            ExtraField::description("My vault"),
            ExtraField {
                field_type: FieldType::Custom("Note".to_string()),
                value: "top level note".to_string(),
            },
        ];
        root
    }

    #[test]
    fn test_export_json_valid() {
        let node = sample_node();
        let result = export_tree(&node, "/tmp/test_export.age", ExportFormat::Json);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with(".json"));
        assert!(std::path::Path::new(&path).exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_xml_valid() {
        let node = sample_node();
        let result = export_tree(&node, "/tmp/test_export.age", ExportFormat::Xml);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with(".xml"));
        assert!(std::path::Path::new(&path).exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_csv_valid() {
        let node = sample_node();
        let result = export_tree(&node, "/tmp/test_export.age", ExportFormat::Csv);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.ends_with(".csv"));
        assert!(std::path::Path::new(&path).exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_json_content() {
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField::description("desc"));
        let result = export_tree(&node, "/tmp/test_content.age", ExportFormat::Json);
        assert!(result.is_ok());
        let path = result.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Root"));
        assert!(content.contains("desc"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_xml_content() {
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField::description("desc"));
        let result = export_tree(&node, "/tmp/test_content.age", ExportFormat::Xml);
        assert!(result.is_ok());
        let path = result.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("<node"));
        assert!(content.contains("Root"));
        assert!(content.contains("desc"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_csv_content() {
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField::description("desc"));
        let result = export_tree(&node, "/tmp/test_content.age", ExportFormat::Csv);
        assert!(result.is_ok());
        let path = result.unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Root"));
        assert!(content.contains("desc"));
        assert!(content.starts_with("path,"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_extension_stripped() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("my_vault.age");
        let node = Node::new("X");
        let result = export_tree(&node, &vault.to_string_lossy(), ExportFormat::Json);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert_eq!(
            std::path::Path::new(&path).file_name().unwrap(),
            "my_vault.json"
        );
    }

    #[test]
    fn test_export_no_extension() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("my_vault");
        let node = Node::new("X");
        let result = export_tree(&node, &vault.to_string_lossy(), ExportFormat::Json);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.contains("my_vault.json"));
    }

    #[test]
    fn test_export_error_on_bad_path() {
        let node = Node::new("X");
        let result = export_tree(
            &node,
            "/nonexistent_dir_xyz123/test.age",
            ExportFormat::Json,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FileHandlingError::Io(_)));
    }

    #[cfg(unix)]
    #[test]
    fn export_is_owner_only_even_over_a_permissive_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault.age");
        let vault = vault.to_string_lossy().to_string();
        export_tree(&Node::new("X"), &vault, ExportFormat::Json).unwrap();

        // Pre-existing exports may carry a group/world-readable mode; the next
        // export must not inherit it, because it holds decrypted secrets.
        let out = dir.path().join("vault.json");
        std::fs::set_permissions(&out, std::fs::Permissions::from_mode(0o644)).unwrap();

        export_tree(&sample_node(), &vault, ExportFormat::Json).unwrap();

        let mode = std::fs::metadata(&out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "a plaintext export must be owner-only");
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("has,comma"), "\"has,comma\"");
        assert_eq!(csv_escape("has\"quote"), "\"has\"\"quote\"");
        assert_eq!(csv_escape("has\nnewline"), "\"has\nnewline\"");
        assert_eq!(csv_escape("has\rcarriage"), "\"has\rcarriage\"");
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(
            escape_xml("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
        assert_eq!(escape_xml("plain"), "plain");
    }

    #[test]
    fn output_path_keeps_the_directory_and_replaces_the_extension() {
        assert_eq!(
            output_path(Path::new("/tmp/my_vault.age"), ExportFormat::Xml),
            Path::new("/tmp/my_vault.xml")
        );
    }

    #[test]
    fn output_path_is_idempotent() {
        let once = output_path(Path::new("/tmp/backup.txt"), ExportFormat::Json);

        assert_eq!(output_path(&once, ExportFormat::Json), once);
    }

    #[test]
    fn output_path_of_a_bare_name_stays_relative() {
        assert_eq!(
            output_path(Path::new("vault"), ExportFormat::Csv),
            Path::new("vault.csv")
        );
    }

    #[test]
    fn suggest_export_name_uses_the_secret_file_stem() {
        assert_eq!(
            suggest_export_name(Some(Path::new("/tmp/vault.age")), None, ExportFormat::Json),
            "vault.json"
        );
    }

    #[test]
    fn suggest_export_name_appends_a_sanitised_node_name() {
        assert_eq!(
            suggest_export_name(
                Some(Path::new("/tmp/vault.age")),
                Some("My Email/Work"),
                ExportFormat::Csv
            ),
            "vault_My_Email_Work.csv"
        );
    }

    #[test]
    fn suggest_export_name_sanitises_a_newline_in_the_secret_file_stem() {
        assert_eq!(
            suggest_export_name(
                Some(Path::new("/tmp/va\nult.age")),
                None,
                ExportFormat::Json
            ),
            "va_ult.json"
        );
    }

    #[test]
    fn suggest_export_name_falls_back_without_a_secret_file() {
        assert_eq!(
            suggest_export_name(None, Some("Login"), ExportFormat::Json),
            "export_Login.json"
        );
    }

    #[test]
    fn export_refuses_to_overwrite_a_secret_file() {
        let dir = tempfile::tempdir().unwrap();
        // A secret file is recognised by its contents, so it may carry any
        // name — even one this very export would produce.
        let vault_path = dir.path().join("vault.json");
        let vault_path = vault_path.to_string_lossy().to_string();
        crate::file_handling::create_secret_file(&vault_path, "pw").unwrap();

        let result = export_tree(&Node::new("X"), &vault_path, ExportFormat::Json);

        assert!(matches!(
            result,
            Err(FileHandlingError::ExportOverwritesSecret { .. })
        ));
        // Untouched: it still decrypts with its master password.
        let data = crate::file_handling::validate_encrypted_file(&vault_path).unwrap();
        assert!(crate::file_handling::decrypt_to_node(&data, "pw").is_ok());
    }
}
