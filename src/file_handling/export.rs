use std::path::Path;

use crate::file_handling::{FileHandlingError, node_to_json};
use crate::tree::node::Node;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExportFormat {
    Json,
    Xml,
    Csv,
}

pub fn export_tree(
    node: &Node,
    file_path: &str,
    format: ExportFormat,
) -> Result<String, FileHandlingError> {
    let path = Path::new(file_path);
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export");
    let parent = path
        .parent()
        .map_or_else(|| ".", |p| p.to_str().unwrap_or("."));
    let out_name = match format {
        ExportFormat::Json => format!("{stem}.json"),
        ExportFormat::Xml => format!("{stem}.xml"),
        ExportFormat::Csv => format!("{stem}.csv"),
    };
    let out_path = Path::new(parent).join(&out_name);
    let out_str = out_path.to_string_lossy().to_string();

    let data = match format {
        ExportFormat::Json => node_to_json(node).map_err(|e| {
            FileHandlingError::ExportError(format!("json serialisation failed: {e}"))
        })?,
        ExportFormat::Xml => node_to_xml(node),
        ExportFormat::Csv => node_to_csv(node),
    };

    std::fs::write(&out_path, &data).map_err(FileHandlingError::Io)?;

    Ok(out_str)
}

fn node_to_xml(node: &Node) -> String {
    let mut buf = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    write_xml_node(&mut buf, node, 0);
    buf
}

fn write_xml_node(buf: &mut String, node: &Node, depth: usize) {
    let indent = "  ".repeat(depth);
    let name_esc = escape_xml(&node.name);
    let desc_esc = escape_xml(&node.description);

    buf.push_str(&format!(
        "{indent}<node name=\"{name_esc}\" description=\"{desc_esc}\">\n"
    ));

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

fn node_to_csv(node: &Node) -> String {
    let mut buf = String::from("path,name,description,field_type,field_value\n");
    write_csv_rows(&mut buf, node, String::new());
    buf
}

fn write_csv_rows(buf: &mut String, node: &Node, parent_path: String) {
    let current_name = csv_escape(&node.name);
    let current_path = if parent_path.is_empty() {
        current_name.clone()
    } else {
        format!("{} > {}", parent_path, current_name)
    };

    if node.extra_fields.is_empty() {
        let desc = csv_escape(&node.description);
        buf.push_str(&format!(
            "{},{},{},\n",
            csv_escape(&current_path),
            current_name,
            desc,
        ));
    } else {
        for ef in &node.extra_fields {
            let desc = csv_escape(&node.description);
            let ft = csv_escape(&ef.field_type.wire_label());
            let val = csv_escape(&ef.value);
            buf.push_str(&format!(
                "{},{},{},{},{}\n",
                csv_escape(&current_path),
                current_name,
                desc,
                ft,
                val,
            ));
        }
    }

    for child in &node.children {
        write_csv_rows(buf, child, current_path.clone());
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
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
        let mut child = Node::new("Login", "My email login");
        child.extra_fields = vec![
            ExtraField {
                field_type: FieldType::Login,
                value: "user@example.com".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "s3cret".to_string(),
            },
        ];
        let mut root = Node::with_children("Secrets", "My vault", vec![child]);
        root.extra_fields = vec![ExtraField {
            field_type: FieldType::Custom("Note".to_string()),
            value: "top level note".to_string(),
        }];
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
        let node = Node::new("Root", "desc");
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
        let node = Node::new("Root", "desc");
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
        let node = Node::new("Root", "desc");
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
        let node = Node::new("X", "");
        let result = export_tree(&node, "/tmp/my_vault.age", ExportFormat::Json);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert_eq!(
            std::path::Path::new(&path).file_name().unwrap(),
            "my_vault.json"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_no_extension() {
        let node = Node::new("X", "");
        let result = export_tree(&node, "/tmp/my_vault", ExportFormat::Json);
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.contains("my_vault.json"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_export_error_on_bad_path() {
        let node = Node::new("X", "");
        let result = export_tree(
            &node,
            "/nonexistent_dir_xyz123/test.age",
            ExportFormat::Json,
        );
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FileHandlingError::Io(_)));
    }

    #[test]
    fn test_csv_escape() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("has,comma"), "\"has,comma\"");
        assert_eq!(csv_escape("has\"quote"), "\"has\"\"quote\"");
        assert_eq!(csv_escape("has\nnewline"), "\"has\nnewline\"");
    }

    #[test]
    fn test_xml_escape() {
        assert_eq!(
            escape_xml("a&b<c>d\"e'f"),
            "a&amp;b&lt;c&gt;d&quot;e&apos;f"
        );
        assert_eq!(escape_xml("plain"), "plain");
    }
}
