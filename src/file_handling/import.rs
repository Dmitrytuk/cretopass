use std::path::Path;

use crate::file_handling::FileHandlingError;
use crate::tree::node::{ExtraField, FieldType, Node};
use uuid::Uuid;

/// Maximum size (in bytes) of a file accepted for import.
const MAX_IMPORT_SIZE: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImportFormat {
    Json,
    Xml,
    Csv,
}

impl ImportFormat {
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path).extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "json" => Some(ImportFormat::Json),
            "xml" => Some(ImportFormat::Xml),
            "csv" => Some(ImportFormat::Csv),
            _ => None,
        }
    }
}

pub fn import_tree(path: &str) -> Result<Node, FileHandlingError> {
    let format = ImportFormat::from_path(path).ok_or_else(|| {
        FileHandlingError::ImportError(format!("unsupported file extension: {path}"))
    })?;

    let meta = std::fs::metadata(path).map_err(FileHandlingError::Io)?;
    if meta.len() > MAX_IMPORT_SIZE {
        return Err(FileHandlingError::FileTooLarge {
            size: meta.len(),
            max: MAX_IMPORT_SIZE,
        });
    }

    let data = std::fs::read_to_string(path).map_err(FileHandlingError::Io)?;

    match format {
        ImportFormat::Json => import_json(&data),
        ImportFormat::Xml => import_xml(&data),
        ImportFormat::Csv => import_csv(&data),
    }
}

fn import_json(data: &str) -> Result<Node, FileHandlingError> {
    serde_json::from_str(data).map_err(FileHandlingError::Serde)
}

/// Maximum XML nesting depth accepted during import, guarding against
/// stack overflow from maliciously deep files.
const MAX_XML_DEPTH: usize = 256;

fn import_xml(data: &str) -> Result<Node, FileHandlingError> {
    let trimmed = data.trim();
    // Skip XML declaration if present
    let body = if trimmed.starts_with("<?xml") {
        let end = trimmed
            .find("?>")
            .ok_or_else(|| FileHandlingError::ImportError("unterminated XML declaration".into()))?;
        trimmed[end + 2..].trim()
    } else {
        trimmed
    };

    let (node, _rest) = parse_xml_node(body, 0)?;
    Ok(node)
}

fn parse_xml_node(input: &str, depth: usize) -> Result<(Node, &str), FileHandlingError> {
    if depth > MAX_XML_DEPTH {
        return Err(FileHandlingError::ImportError(
            "XML nesting too deep".into(),
        ));
    }
    let input = input.trim_start();

    if !input.starts_with("<node") {
        return Err(FileHandlingError::ImportError("expected <node".into()));
    }

    let tag_end = input
        .find('>')
        .ok_or_else(|| FileHandlingError::ImportError("expected > in <node".into()))?;
    let attrs_str = &input[5..tag_end];

    let attrs = extract_xml_attrs(attrs_str);
    let name = attrs
        .iter()
        .find(|(k, _)| k == "name")
        .map(|(_, v)| unescape_xml(v))
        .ok_or_else(|| FileHandlingError::ImportError("missing name attribute".into()))?;
    let description = attrs
        .iter()
        .find(|(k, _)| k == "description")
        .map(|(_, v)| unescape_xml(v))
        .ok_or_else(|| FileHandlingError::ImportError("missing description attribute".into()))?;

    let mut remaining = input[tag_end + 1..].trim_start();

    // Parse <extra_fields>
    let ef_open = "<extra_fields>";
    let ef_close = "</extra_fields>";
    if !remaining.starts_with(ef_open) {
        return Err(FileHandlingError::ImportError(
            "expected <extra_fields>".into(),
        ));
    }
    remaining = &remaining[ef_open.len()..];

    let ef_end = remaining
        .find(ef_close)
        .ok_or_else(|| FileHandlingError::ImportError("expected </extra_fields>".into()))?;
    let ef_content = &remaining[..ef_end];
    remaining = &remaining[ef_end + ef_close.len()..];

    let extra_fields = parse_xml_fields(ef_content)?;

    remaining = remaining.trim_start();

    // Parse optional <children>
    let mut children = Vec::new();
    if remaining.starts_with("<children>") {
        remaining = &remaining["<children>".len()..];
        loop {
            remaining = remaining.trim_start();
            if remaining.starts_with("</children>") {
                remaining = &remaining["</children>".len()..];
                break;
            }
            let (child, rest) = parse_xml_node(remaining, depth + 1)?;
            children.push(child);
            remaining = rest;
        }
        remaining = remaining.trim_start();
    }

    // Expect </node>
    if !remaining.starts_with("</node>") {
        return Err(FileHandlingError::ImportError("expected </node>".into()));
    }
    remaining = &remaining["</node>".len()..];

    let node = Node {
        id: Uuid::new_v4(),
        name,
        description,
        extra_fields,
        children,
    };

    Ok((node, remaining))
}

fn extract_xml_attrs(s: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let mut remaining = s.trim();
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        let eq_pos = match remaining.find('=') {
            Some(p) => p,
            None => break,
        };
        let key = remaining[..eq_pos].trim().to_string();
        remaining = &remaining[eq_pos + 1..];
        remaining = remaining.trim_start();
        if remaining.starts_with('"') {
            remaining = &remaining[1..];
            if let Some(end) = remaining.find('"') {
                let value = remaining[..end].to_string();
                attrs.push((key, value));
                remaining = &remaining[end + 1..];
            }
        }
    }
    attrs
}

fn parse_xml_fields(input: &str) -> Result<Vec<ExtraField>, FileHandlingError> {
    let mut fields = Vec::new();
    let mut remaining = input.trim();
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.starts_with("</") || remaining.is_empty() {
            break;
        }
        if !remaining.starts_with("<field") {
            return Err(FileHandlingError::ImportError("expected <field".into()));
        }
        remaining = &remaining["<field".len()..];

        let tag_end = remaining
            .find('>')
            .ok_or_else(|| FileHandlingError::ImportError("expected > in <field".into()))?;
        let attrs_str = &remaining[..tag_end];
        remaining = &remaining[tag_end + 1..];

        let attrs = extract_xml_attrs(attrs_str);
        let ft_str = attrs
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| FileHandlingError::ImportError("missing type in <field>".into()))?;
        let value = attrs
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| unescape_xml(v))
            .ok_or_else(|| FileHandlingError::ImportError("missing value in <field>".into()))?;

        let field_type = FieldType::from_wire_label(ft_str);

        fields.push(ExtraField { field_type, value });
    }
    Ok(fields)
}

fn unescape_xml(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

#[derive(Default)]
struct PathData {
    name: String,
    description: String,
    extra_fields: Vec<ExtraField>,
}

fn import_csv(data: &str) -> Result<Node, FileHandlingError> {
    let mut lines: Vec<&str> = data.lines().collect();
    if lines.is_empty() {
        return Err(FileHandlingError::ImportError("empty CSV file".into()));
    }

    // Skip header (tolerate a trailing CR from CRLF line endings).
    if lines[0].trim_end_matches('\r').trim().starts_with("path,") {
        lines.remove(0);
    }

    if lines.is_empty() {
        return Err(FileHandlingError::ImportError(
            "CSV file has no data rows".into(),
        ));
    }

    // Parse all rows
    struct Row {
        path: String,
        name: String,
        description: String,
        field_type: Option<String>,
        field_value: String,
    }

    let mut rows: Vec<Row> = Vec::new();
    for line in &lines {
        // Strip a trailing CR left by CRLF line endings.
        let trimmed = line.trim_end_matches('\r').trim();
        if trimmed.is_empty() {
            continue;
        }
        let fields = parse_csv_line(trimmed);
        if fields.len() < 4 {
            return Err(FileHandlingError::ImportError(format!(
                "CSV row has too few fields: {trimmed}"
            )));
        }
        let field_type = if fields.len() >= 5 && !fields[3].is_empty() {
            Some(fields[3].clone())
        } else {
            None
        };
        let field_value = if fields.len() >= 5 {
            fields[4].clone()
        } else {
            String::new()
        };

        rows.push(Row {
            path: fields[0].clone(),
            name: fields[1].clone(),
            description: fields[2].clone(),
            field_type,
            field_value,
        });
    }

    // Group by path: collect all extra fields per path
    // Use a map to track insertion order
    let mut path_map: Vec<(String, PathData)> = Vec::new();
    for row in &rows {
        let path = &row.path;
        if let Some((_, data)) = path_map.iter_mut().find(|(p, _)| p == path) {
            if let Some(ref ft) = row.field_type {
                let field_type = FieldType::from_wire_label(ft);
                data.extra_fields.push(ExtraField {
                    field_type,
                    value: row.field_value.clone(),
                });
            }
        } else {
            let mut extra_fields = Vec::new();
            if let Some(ref ft) = row.field_type {
                let field_type = FieldType::from_wire_label(ft);
                extra_fields.push(ExtraField {
                    field_type,
                    value: row.field_value.clone(),
                });
            }
            path_map.push((
                path.clone(),
                PathData {
                    name: row.name.clone(),
                    description: row.description.clone(),
                    extra_fields,
                },
            ));
        }
    }

    // Build tree from paths
    // Each path is like "Root > Category > Item"
    build_tree_from_paths(&path_map)
}

fn build_tree_from_paths(path_map: &[(String, PathData)]) -> Result<Node, FileHandlingError> {
    #[derive(Default)]
    struct TreeNode {
        key: String,
        name: String,
        description: String,
        extra_fields: Vec<ExtraField>,
        children: Vec<TreeNode>,
    }

    fn into_node(tree: TreeNode) -> Node {
        Node {
            id: Uuid::new_v4(),
            name: tree.name,
            description: tree.description,
            extra_fields: tree.extra_fields,
            children: tree.children.into_iter().map(into_node).collect(),
        }
    }

    let mut roots: Vec<TreeNode> = Vec::new();

    for (path_str, data) in path_map {
        let segments: Vec<&str> = path_str.split(" > ").collect();
        let mut idx_path: Vec<usize> = Vec::new();

        for (seg_idx, seg) in segments.iter().enumerate() {
            let seg_str = seg.to_string();
            let is_last = seg_idx == segments.len() - 1;

            let container = if idx_path.is_empty() {
                &mut roots
            } else {
                let mut c = &mut roots;
                for &i in &idx_path {
                    c = &mut c[i].children;
                }
                c
            };

            let pos = container.iter().position(|n| n.key == seg_str);
            if let Some(pos) = pos {
                if is_last {
                    let node = &mut container[pos];
                    node.name = data.name.clone();
                    node.description = data.description.clone();
                    node.extra_fields = data.extra_fields.clone();
                }
                idx_path.push(pos);
            } else {
                container.push(TreeNode {
                    key: seg_str.clone(),
                    name: if is_last {
                        data.name.clone()
                    } else {
                        seg_str.clone()
                    },
                    description: if is_last {
                        data.description.clone()
                    } else {
                        String::new()
                    },
                    extra_fields: if is_last {
                        data.extra_fields.clone()
                    } else {
                        Vec::new()
                    },
                    children: Vec::new(),
                });
                idx_path.push(container.len() - 1);
            }
        }
    }

    if roots.is_empty() {
        return Err(FileHandlingError::ImportError("no data to import".into()));
    }

    if roots.len() == 1 {
        Ok(into_node(roots.into_iter().next().unwrap()))
    } else {
        let children: Vec<Node> = roots.into_iter().map(into_node).collect();
        Ok(Node {
            id: Uuid::new_v4(),
            name: "Imported".to_string(),
            description: String::new(),
            extra_fields: Vec::new(),
            children,
        })
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars();

    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => in_quotes = true,
            '"' if in_quotes => {
                // Check for escaped quote ""
                if let Some(next) = chars.clone().next() {
                    if next == '"' {
                        chars.next();
                        current.push('"');
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = false;
                }
            }
            ',' if !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_handling::export::{export_tree, ExportFormat};

    fn sample_xml() -> &'static str {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<node name="Secrets" description="My vault">
  <extra_fields>
    <field type="Note" value="top level note"/>
  </extra_fields>
  <children>
    <node name="Login" description="My email login">
      <extra_fields>
        <field type="Login" value="user@example.com"/>
        <field type="Password" value="s3cret"/>
      </extra_fields>
    </node>
    <node name="Server" description="SSH config">
      <extra_fields>
        <field type="Custom:Host" value="example.com"/>
      </extra_fields>
      <children>
        <node name="Key" description="private key path">
          <extra_fields>
            <field type="Password" value="encrypted-key"/>
          </extra_fields>
        </node>
      </children>
    </node>
  </children>
</node>"#
    }

    fn sample_csv() -> &'static str {
        "path,name,description,field_type,field_value\n\
         Secrets,Secrets,My vault,Note,top level note\n\
         Secrets > Login,Login,My email login,Login,user@example.com\n\
         Secrets > Login,Login,My email login,Password,s3cret\n\
         Secrets > Server,Server,SSH config,Custom:Host,example.com\n\
         Secrets > Server > Key,Key,private key path,Password,encrypted-key"
    }

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
        let mut key = Node::new("Key", "private key path");
        key.extra_fields = vec![ExtraField {
            field_type: FieldType::Password,
            value: "encrypted-key".to_string(),
        }];
        let mut server = Node::with_children("Server", "SSH config", vec![key]);
        server.extra_fields = vec![ExtraField {
            field_type: FieldType::Custom("Custom:Host".to_string()),
            value: "example.com".to_string(),
        }];
        let mut root = Node::with_children("Secrets", "My vault", vec![child, server]);
        root.extra_fields = vec![ExtraField {
            field_type: FieldType::Custom("Note".to_string()),
            value: "top level note".to_string(),
        }];
        root
    }

    #[test]
    fn test_import_json_roundtrip() {
        let original = sample_node();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let imported = import_json(&json).unwrap();
        // IDs are regenerated, so compare structure
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.description, original.description);
        assert_eq!(imported.extra_fields.len(), original.extra_fields.len());
        assert_eq!(imported.children.len(), original.children.len());
        // Check extra field values
        assert_eq!(imported.extra_fields[0].value, "top level note");
    }

    #[test]
    fn test_import_xml() {
        let imported = import_xml(sample_xml()).unwrap();
        assert_eq!(imported.name, "Secrets");
        assert_eq!(imported.description, "My vault");
        assert_eq!(imported.extra_fields.len(), 1);
        assert_eq!(imported.extra_fields[0].value, "top level note");
        assert_eq!(imported.children.len(), 2);

        let login = &imported.children[0];
        assert_eq!(login.name, "Login");
        assert_eq!(login.extra_fields.len(), 2);
        assert_eq!(login.extra_fields[0].value, "user@example.com");
        assert_eq!(login.extra_fields[1].value, "s3cret");
        assert!(matches!(login.extra_fields[0].field_type, FieldType::Login));
        assert!(matches!(
            login.extra_fields[1].field_type,
            FieldType::Password
        ));

        let server = &imported.children[1];
        assert_eq!(server.name, "Server");
        assert_eq!(server.extra_fields[0].value, "example.com");
        assert_eq!(server.children.len(), 1);
        assert_eq!(server.children[0].name, "Key");
    }

    #[test]
    fn test_import_csv() {
        let imported = import_csv(sample_csv()).unwrap();
        assert_eq!(imported.name, "Secrets");
        assert_eq!(imported.description, "My vault");
        assert_eq!(imported.children.len(), 2);

        let login = &imported.children[0];
        assert_eq!(login.name, "Login");
        assert_eq!(login.extra_fields.len(), 2);
        assert_eq!(login.extra_fields[0].value, "user@example.com");

        let server = &imported.children[1];
        assert_eq!(server.name, "Server");
        assert_eq!(server.children.len(), 1);
        assert_eq!(server.children[0].name, "Key");
    }

    #[test]
    fn test_import_csv_invalid_short_row() {
        let data = "path,name,desc,ft,fv\nfoo,bar";
        let result = import_csv(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_csv_empty() {
        let result = import_csv("");
        assert!(result.is_err());
    }

    #[test]
    fn test_import_xml_escaped_chars() {
        let xml = r#"<?xml version="1.0"?>
<node name="A &amp; B" description="x &lt; y">
  <extra_fields>
    <field type="Login" value="a&quot;b"/>
  </extra_fields>
</node>"#;
        let node = import_xml(xml).unwrap();
        assert_eq!(node.name, "A & B");
        assert_eq!(node.description, "x < y");
        assert_eq!(node.extra_fields[0].value, "a\"b");
    }

    #[test]
    fn test_import_xml_no_children() {
        let xml = r#"<?xml version="1.0"?>
<node name="Leaf" description="no children">
  <extra_fields>
  </extra_fields>
</node>"#;
        let node = import_xml(xml).unwrap();
        assert_eq!(node.name, "Leaf");
        assert!(node.children.is_empty());
        assert!(node.extra_fields.is_empty());
    }

    #[test]
    fn test_import_xml_no_declaration() {
        let xml = r#"<node name="Root" description="">
  <extra_fields>
  </extra_fields>
</node>"#;
        let node = import_xml(xml).unwrap();
        assert_eq!(node.name, "Root");
    }

    #[test]
    fn test_extract_xml_attrs() {
        let attrs = extract_xml_attrs(r#"name="Root" description="hello world""#);
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].0, "name");
        assert_eq!(attrs[0].1, "Root");
        assert_eq!(attrs[1].0, "description");
        assert_eq!(attrs[1].1, "hello world");
    }

    #[test]
    fn test_parse_csv_line_simple() {
        let fields = parse_csv_line("a,b,c");
        assert_eq!(fields, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_parse_csv_line_quoted() {
        let fields = parse_csv_line(r#"a,"b,c",d"#);
        assert_eq!(fields, vec!["a", "b,c", "d"]);
    }

    #[test]
    fn test_parse_csv_line_escaped_quote() {
        let fields = parse_csv_line(r#"a,"b""c",d"#);
        assert_eq!(fields, vec!["a", r#"b"c"#, "d"]);
    }

    #[test]
    fn test_import_format_from_path() {
        assert_eq!(
            ImportFormat::from_path("file.json"),
            Some(ImportFormat::Json)
        );
        assert_eq!(ImportFormat::from_path("file.xml"), Some(ImportFormat::Xml));
        assert_eq!(ImportFormat::from_path("file.csv"), Some(ImportFormat::Csv));
        assert_eq!(ImportFormat::from_path("file.age"), None);
        assert_eq!(ImportFormat::from_path("file"), None);
    }

    #[test]
    fn test_import_from_file() {
        let path = format!("/tmp/test_import_roundtrip_{}.json", uuid::Uuid::new_v4());
        let original = sample_node();
        let json = serde_json::to_string_pretty(&original).unwrap();
        std::fs::write(&path, &json).unwrap();

        let imported = import_tree(&path).unwrap();
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.children.len(), original.children.len());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_import_from_file_unsupported() {
        let result = import_tree("/tmp/test.unsupported");
        assert!(result.is_err());
    }

    #[test]
    fn test_import_xml_empty_extra_fields() {
        let xml = r#"<?xml version="1.0"?>
<node name="Empty" description="no extra fields">
  <extra_fields>
  </extra_fields>
  <children>
    <node name="Child" description="leaf">
      <extra_fields>
      </extra_fields>
    </node>
  </children>
</node>"#;
        let node = import_xml(xml).unwrap();
        assert_eq!(node.name, "Empty");
        assert_eq!(node.children.len(), 1);
        assert!(node.children[0].extra_fields.is_empty());
    }

    #[test]
    fn test_csv_with_values_all_empty() {
        let data = "path,name,description,field_type,field_value\n\
                     Root,Root,desc,,\n\
                     Root,Root,desc,,,\n";
        let node = import_csv(data).unwrap();
        assert_eq!(node.name, "Root");
        assert_eq!(node.description, "desc");
        assert!(node.extra_fields.is_empty());
    }

    #[test]
    fn test_import_csv_multiple_roots_wrapped() {
        let data = "path,name,description,field_type,field_value\n\
                     Alpha,Alpha,A,,,\n\
                     Beta,Beta,B,,,\n";
        let node = import_csv(data).unwrap();
        assert_eq!(node.name, "Imported");
        assert_eq!(node.children.len(), 2);
        assert_eq!(node.children[0].name, "Alpha");
        assert_eq!(node.children[1].name, "Beta");
    }

    #[test]
    fn test_csv_escape_roundtrip() {
        let data = r#"path,name,description,field_type,field_value
"Root","Name with, comma","Desc with "" quote",,
"Root > Child","Child","desc",Login,"va,lue"""#;
        let node = import_csv(data).unwrap();
        assert_eq!(node.name, "Name with, comma");
        assert_eq!(node.description, "Desc with \" quote");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].extra_fields[0].value, "va,lue\"");
    }

    // ── End-to-end roundtrip tests ──

    fn roundtrip_node(node: &Node) -> Node {
        // Also test XML roundtrip via temp file
        let xml_path = format!("/tmp/roundtrip_xml_{}.xml", uuid::Uuid::new_v4());
        export_tree(node, &xml_path, ExportFormat::Xml).unwrap();
        let imported = import_tree(&xml_path).unwrap();
        let _ = std::fs::remove_file(&xml_path);
        imported
    }

    fn assert_nodes_match(original: &Node, imported: &Node) {
        assert_eq!(original.name, imported.name);
        assert_eq!(original.description, imported.description);
        assert_eq!(original.extra_fields.len(), imported.extra_fields.len());
        assert_eq!(original.children.len(), imported.children.len());
        for (a, b) in original.children.iter().zip(imported.children.iter()) {
            assert_nodes_match(a, b);
        }
    }

    #[test]
    fn test_xml_roundtrip_simple() {
        let original = Node::new("Test", "simple roundtrip");
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
    }

    #[test]
    fn test_xml_roundtrip_nested() {
        let mut child = Node::new("Login", "My login");
        child.extra_fields = vec![
            ExtraField {
                field_type: FieldType::Login,
                value: "user".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "pass".to_string(),
            },
        ];
        let original = Node::with_children("Root", "Vault", vec![child]);
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
        assert_eq!(imported.children[0].extra_fields.len(), 2);
        assert_eq!(imported.children[0].extra_fields[0].value, "user");
    }

    #[test]
    fn test_xml_roundtrip_deeply_nested() {
        let leaf = Node::new("Leaf", "deep");
        let mid = Node::with_children("Middle", "mid", vec![leaf]);
        let original = Node::with_children("Root", "root", vec![mid]);
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
        assert_eq!(imported.children[0].children[0].name, "Leaf");
    }

    #[test]
    fn test_xml_roundtrip_special_chars() {
        let mut original = Node::new("A & B <C>", r#"description "with" quotes'apos"#);
        original.extra_fields = vec![ExtraField {
            field_type: FieldType::Custom("<tag>".to_string()),
            value: "a&b".to_string(),
        }];
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
    }

    #[test]
    fn test_csv_roundtrip() {
        let mut child = Node::new("Login", "My login");
        child.extra_fields = vec![
            ExtraField {
                field_type: FieldType::Login,
                value: "user".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "pass".to_string(),
            },
        ];
        let original = Node::with_children("Root", "Vault", vec![child]);

        let csv_path = format!("/tmp/roundtrip_csv_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
    }

    #[test]
    fn test_csv_roundtrip_multiple_extra_fields() {
        let mut child = Node::new("Entry", "Login creds");
        child.extra_fields = vec![
            ExtraField {
                field_type: FieldType::Login,
                value: "admin".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "secret123".to_string(),
            },
            ExtraField {
                field_type: FieldType::Custom("URL".to_string()),
                value: "https://example.com".to_string(),
            },
        ];
        let original = Node::with_children("Secrets", "", vec![child]);

        let csv_path = format!("/tmp/roundtrip_csv2_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
        assert_eq!(imported.children[0].extra_fields.len(), 3);
        assert_eq!(imported.children[0].extra_fields[0].value, "admin");
    }

    #[test]
    fn test_json_roundtrip_via_import_tree() {
        let original = Node::with_children(
            "Root",
            "desc",
            vec![Node::new("Child1", "leaf1"), Node::new("Child2", "leaf2")],
        );
        let json_path = format!("/tmp/roundtrip_json_{}.json", uuid::Uuid::new_v4());
        export_tree(&original, &json_path, ExportFormat::Json).unwrap();
        let imported = import_tree(&json_path).unwrap();
        let _ = std::fs::remove_file(&json_path);

        assert_nodes_match(&original, &imported);
    }

    #[test]
    fn test_import_tree_unsupported_extension() {
        let result = import_tree("/tmp/test.xyz");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            FileHandlingError::ImportError(_)
        ));
    }
}
