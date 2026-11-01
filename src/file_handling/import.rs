use std::io::Read;

use super::FileHandlingError;
use super::atomic;
use super::error::ImportSyntax;
use super::file_process::MAX_FILE_SIZE;
use super::format::Format;
use crate::tree::node::{ExtraField, FieldType, Node, migrate_legacy_description};
use uuid::Uuid;

/// The formats an imported document may use.
pub type ImportFormat = Format;

/// Builds a [`FileHandlingError::ImportSyntax`] from `reason`.
fn syntax(reason: ImportSyntax) -> FileHandlingError {
    FileHandlingError::ImportSyntax(reason)
}

/// Reads `path` and builds a tree from it, choosing the parser by extension.
///
/// # Errors
///
/// Returns [`FileHandlingError::UnsupportedImportFormat`] for an extension
/// this module cannot parse, [`FileHandlingError::FileTooLarge`] when the file
/// exceeds the shared size cap, [`FileHandlingError::Io`] when it cannot be
/// read, and a [`FileHandlingError::ImportSyntax`] (or
/// [`FileHandlingError::CsvRowTooFewFields`]) when the document is malformed.
pub fn import_tree(path: &str) -> Result<Node, FileHandlingError> {
    let format = ImportFormat::from_path(path)
        .ok_or_else(|| FileHandlingError::UnsupportedImportFormat(path.to_string()))?;

    let file = atomic::open_for_read(std::path::Path::new(path))?;
    let meta = file.metadata().map_err(FileHandlingError::Io)?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(FileHandlingError::FileTooLarge {
            size: meta.len(),
            max: MAX_FILE_SIZE,
        });
    }

    // Read through a bounded reader so a file that grows (or is swapped for a
    // stream) between the metadata check and the read cannot exhaust memory.
    // The cap is checked on the raw bytes before UTF-8 validation so an
    // over-sized file is always reported as `FileTooLarge`.
    let mut bytes = Vec::new();
    file.take(MAX_FILE_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(FileHandlingError::Io)?;
    if bytes.len() as u64 > MAX_FILE_SIZE {
        return Err(FileHandlingError::FileTooLarge {
            size: bytes.len() as u64,
            max: MAX_FILE_SIZE,
        });
    }
    let data = String::from_utf8(bytes).map_err(|e| {
        FileHandlingError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;

    match format {
        Format::Json => import_json(&data),
        Format::Xml => import_xml(&data),
        Format::Csv => import_csv(&data),
    }
}

fn import_json(data: &str) -> Result<Node, FileHandlingError> {
    serde_json::from_str(data).map_err(FileHandlingError::Serde)
}

/// Maximum XML nesting depth accepted during import, guarding against
/// stack overflow from maliciously deep files.
const MAX_XML_DEPTH: usize = 256;

/// Maximum number of `>`-separated segments accepted in a CSV path,
/// mirroring [`MAX_XML_DEPTH`] so both importers bound their tree depth.
const MAX_CSV_PATH_DEPTH: usize = 256;

fn import_xml(data: &str) -> Result<Node, FileHandlingError> {
    let trimmed = data.trim();
    // Skip XML declaration if present
    let body = if trimmed.starts_with("<?xml") {
        let end = trimmed
            .find("?>")
            .ok_or_else(|| syntax(ImportSyntax::UnterminatedXmlDeclaration))?;
        trimmed[end + 2..].trim()
    } else {
        trimmed
    };

    let (node, _rest) = parse_xml_node(body, 0)?;
    Ok(node)
}

fn parse_xml_node(input: &str, depth: usize) -> Result<(Node, &str), FileHandlingError> {
    if depth > MAX_XML_DEPTH {
        return Err(syntax(ImportSyntax::XmlNestingTooDeep));
    }
    let input = input.trim_start();

    if !input.starts_with("<node") {
        return Err(syntax(ImportSyntax::ExpectedNodeStart));
    }

    let tag_end = input
        .find('>')
        .ok_or_else(|| syntax(ImportSyntax::ExpectedNodeTagEnd))?;
    let attrs_str = &input[5..tag_end];

    let attrs = extract_xml_attrs(attrs_str)?;
    let name = attrs
        .iter()
        .find(|(k, _)| k == "name")
        .map(|(_, v)| unescape_xml(v))
        .ok_or_else(|| syntax(ImportSyntax::MissingNodeName))?;
    let description = attrs
        .iter()
        .find(|(k, _)| k == "description")
        .map(|(_, v)| unescape_xml(v));

    let mut remaining = input[tag_end + 1..].trim_start();

    // Parse <extra_fields>
    let ef_open = "<extra_fields>";
    let ef_close = "</extra_fields>";
    if !remaining.starts_with(ef_open) {
        return Err(syntax(ImportSyntax::ExpectedExtraFieldsStart));
    }
    remaining = &remaining[ef_open.len()..];

    let ef_end = remaining
        .find(ef_close)
        .ok_or_else(|| syntax(ImportSyntax::ExpectedExtraFieldsEnd))?;
    let ef_content = &remaining[..ef_end];
    remaining = &remaining[ef_end + ef_close.len()..];

    let mut extra_fields = parse_xml_fields(ef_content)?;
    migrate_legacy_description(description.as_deref(), &mut extra_fields);

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
        return Err(syntax(ImportSyntax::ExpectedNodeEnd));
    }
    remaining = &remaining["</node>".len()..];

    let node = Node {
        id: Uuid::new_v4(),
        name,
        extra_fields,
        children,
    };

    Ok((node, remaining))
}

fn extract_xml_attrs(s: &str) -> Result<Vec<(String, String)>, FileHandlingError> {
    let mut attrs = Vec::new();
    let mut remaining = s.trim();
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.is_empty() {
            break;
        }
        // A self-closing tag leaves a lone `/` after the last attribute.
        if let Some(rest) = remaining.strip_prefix('/') {
            remaining = rest;
            continue;
        }
        let eq_pos = remaining
            .find('=')
            .ok_or_else(|| syntax(ImportSyntax::MalformedAttribute))?;
        let key = remaining[..eq_pos].trim().to_string();
        remaining = remaining[eq_pos + 1..].trim_start();
        let after_quote = remaining
            .strip_prefix('"')
            .ok_or_else(|| syntax(ImportSyntax::MalformedAttribute))?;
        let end = after_quote
            .find('"')
            .ok_or_else(|| syntax(ImportSyntax::MalformedAttribute))?;
        attrs.push((key, after_quote[..end].to_string()));
        remaining = &after_quote[end + 1..];
    }
    Ok(attrs)
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
            return Err(syntax(ImportSyntax::ExpectedFieldStart));
        }
        remaining = &remaining["<field".len()..];

        let tag_end = remaining
            .find('>')
            .ok_or_else(|| syntax(ImportSyntax::ExpectedFieldTagEnd))?;
        let attrs_str = &remaining[..tag_end];
        remaining = &remaining[tag_end + 1..];

        let attrs = extract_xml_attrs(attrs_str)?;
        let field_type_label = attrs
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| v.as_str())
            .ok_or_else(|| syntax(ImportSyntax::MissingFieldType))?;
        let value = attrs
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| unescape_xml(v))
            .ok_or_else(|| syntax(ImportSyntax::MissingFieldValue))?;

        let field_type = FieldType::from_wire_label(field_type_label);

        fields.push(ExtraField { field_type, value });
    }
    Ok(fields)
}

/// Decodes the five predefined XML entities in a single pass.
///
/// A single pass is essential: replacing the entities one after another would
/// decode `&amp;lt;` — the correct escape of a literal `&lt;` — twice, turning
/// it into `<`. An unknown entity is left verbatim.
fn unescape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];
        match decode_entity(tail) {
            Some((decoded, consumed)) => {
                out.push(decoded);
                rest = &tail[consumed..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Decodes the entity at the start of `s`, returning it and the number of
/// bytes consumed.
fn decode_entity(s: &str) -> Option<(char, usize)> {
    for (name, decoded) in [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
    ] {
        if s.starts_with(name) {
            return Some((decoded, name.len()));
        }
    }
    None
}

#[derive(Default)]
struct PathData {
    name: String,
    description: Option<String>,
    extra_fields: Vec<ExtraField>,
}

/// Maps a CSV header to the column indices of the fields we understand.
///
/// Columns are located by name, so a reordered or quoted header is understood.
/// `path` and `name` are required; the legacy `description` column and the
/// `field_type`/`field_value` pair are optional.
struct CsvColumns {
    path: usize,
    name: usize,
    description: Option<usize>,
    field_type: Option<usize>,
    field_value: Option<usize>,
}

impl CsvColumns {
    /// Locates the columns by header name.
    ///
    /// Both the legacy layout (`path,name,description,field_type,field_value`)
    /// and the current one (`path,name,field_type,field_value`) are accepted.
    ///
    /// # Errors
    ///
    /// Returns [`ImportSyntax::MissingCsvHeader`] when the header has no `path`
    /// or `name` column, i.e. the first record is not a recognisable header.
    fn from_header(header: &[String]) -> Result<Self, FileHandlingError> {
        let find = |column: &str| header.iter().position(|c| c.trim() == column);
        Ok(Self {
            path: find("path").ok_or_else(|| syntax(ImportSyntax::MissingCsvHeader))?,
            name: find("name").ok_or_else(|| syntax(ImportSyntax::MissingCsvHeader))?,
            description: find("description"),
            field_type: find("field_type"),
            field_value: find("field_value"),
        })
    }
}

fn import_csv(data: &str) -> Result<Node, FileHandlingError> {
    let mut records = parse_csv_records(data);
    if records.is_empty() {
        return Err(syntax(ImportSyntax::EmptyCsv));
    }

    let (_, header) = records.remove(0);
    let cols = CsvColumns::from_header(&header)?;
    if records.is_empty() {
        return Err(syntax(ImportSyntax::NoCsvDataRows));
    }

    let get = |fields: &[String], idx: Option<usize>| -> Option<String> {
        idx.and_then(|i| fields.get(i)).cloned()
    };

    // Group by path: collect all extra fields per path
    // Use a map to track insertion order
    let mut path_map: Vec<(String, PathData)> = Vec::new();
    for (line, fields) in &records {
        if fields.iter().all(|field| field.trim().is_empty()) {
            continue;
        }
        if fields.len() <= cols.path || fields.len() <= cols.name {
            return Err(FileHandlingError::CsvRowTooFewFields { line: *line });
        }
        let path = fields[cols.path].clone();
        let name = fields[cols.name].clone();
        let description = get(fields, cols.description).filter(|d| !d.is_empty());
        let field_type = get(fields, cols.field_type).filter(|ft| !ft.is_empty());
        let field_value = get(fields, cols.field_value).unwrap_or_default();

        if let Some((_, data)) = path_map.iter_mut().find(|(p, _)| *p == path) {
            if let Some(ft) = field_type {
                data.extra_fields.push(ExtraField {
                    field_type: FieldType::from_wire_label(&ft),
                    value: field_value,
                });
            }
        } else {
            let mut extra_fields = Vec::new();
            if let Some(ft) = field_type {
                extra_fields.push(ExtraField {
                    field_type: FieldType::from_wire_label(&ft),
                    value: field_value,
                });
            }
            path_map.push((
                path,
                PathData {
                    name,
                    description,
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
        description: Option<String>,
        extra_fields: Vec<ExtraField>,
        children: Vec<TreeNode>,
    }

    fn into_node(tree: TreeNode) -> Node {
        let mut extra_fields = tree.extra_fields;
        migrate_legacy_description(tree.description.as_deref(), &mut extra_fields);
        Node {
            id: Uuid::new_v4(),
            name: tree.name,
            extra_fields,
            children: tree.children.into_iter().map(into_node).collect(),
        }
    }

    let mut roots: Vec<TreeNode> = Vec::new();

    for (path_str, data) in path_map {
        let segments: Vec<&str> = path_str.split(" > ").collect();
        if segments.len() > MAX_CSV_PATH_DEPTH {
            return Err(syntax(ImportSyntax::CsvPathTooDeep));
        }
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
                        None
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
        return Err(syntax(ImportSyntax::NoTree));
    }

    if roots.len() == 1 {
        Ok(into_node(roots.into_iter().next().unwrap()))
    } else {
        let children: Vec<Node> = roots.into_iter().map(into_node).collect();
        Ok(Node {
            id: Uuid::new_v4(),
            name: "Imported".to_string(),
            extra_fields: Vec::new(),
            children,
        })
    }
}

/// Splits CSV `data` into records, honouring quoted fields that may span
/// multiple physical lines and `""`-escaped quotes.
///
/// Each record is paired with the 1-based physical line it starts on, so a
/// parse error can be reported by position without echoing any value.
fn parse_csv_records(data: &str) -> Vec<(usize, Vec<String>)> {
    let mut records = Vec::new();
    let mut fields: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut line = 1usize;
    let mut record_line = 1usize;
    let mut chars = data.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' if !in_quotes => in_quotes = true,
            '"' if in_quotes => {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    current.push('"');
                } else {
                    in_quotes = false;
                }
            }
            ',' if !in_quotes => fields.push(std::mem::take(&mut current)),
            '\n' if !in_quotes => {
                fields.push(std::mem::take(&mut current));
                records.push((record_line, std::mem::take(&mut fields)));
                line += 1;
                record_line = line;
            }
            '\r' if !in_quotes => {
                // A CRLF pair terminates one record: consume the LF here so it
                // does not start an empty record of its own.
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                fields.push(std::mem::take(&mut current));
                records.push((record_line, std::mem::take(&mut fields)));
                line += 1;
                record_line = line;
            }
            '\n' => {
                // A newline inside a quoted field is data.
                current.push('\n');
                line += 1;
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() || !fields.is_empty() {
        fields.push(current);
        records.push((record_line, fields));
    }

    records
}

/// Parses a single CSV line into its fields. Used for headers and tests.
#[cfg(test)]
fn parse_csv_line(line: &str) -> Vec<String> {
    parse_csv_records(line)
        .into_iter()
        .next()
        .map(|(_, fields)| fields)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_handling::{ExportFormat, export_tree};

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
        let mut key = Node::new("Key");
        key.extra_fields = vec![
            ExtraField::description("private key path"),
            ExtraField {
                field_type: FieldType::Password,
                value: "encrypted-key".to_string(),
            },
        ];
        let mut server = Node::with_children("Server", vec![key]);
        server.extra_fields = vec![
            ExtraField::description("SSH config"),
            ExtraField {
                field_type: FieldType::Custom("Custom:Host".to_string()),
                value: "example.com".to_string(),
            },
        ];
        let mut root = Node::with_children("Secrets", vec![child, server]);
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
    fn test_import_json_roundtrip() {
        let original = sample_node();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let imported = import_json(&json).unwrap();
        // IDs are regenerated, so compare structure
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.extra_fields.len(), original.extra_fields.len());
        assert_eq!(imported.children.len(), original.children.len());
        // Check extra field values
        assert_eq!(imported.extra_fields[0].value, "My vault");
        assert_eq!(imported.extra_fields[1].value, "top level note");
    }

    #[test]
    fn test_import_xml() {
        let imported = import_xml(sample_xml()).unwrap();
        assert_eq!(imported.name, "Secrets");
        // The legacy description attribute migrates to a leading extra field.
        assert_eq!(imported.extra_fields.len(), 2);
        assert_eq!(imported.extra_fields[0].field_type.label(), "Description");
        assert_eq!(imported.extra_fields[0].value, "My vault");
        assert_eq!(imported.extra_fields[1].value, "top level note");
        assert_eq!(imported.children.len(), 2);

        let login = &imported.children[0];
        assert_eq!(login.name, "Login");
        assert_eq!(login.extra_fields.len(), 3);
        assert_eq!(login.extra_fields[0].value, "My email login");
        assert_eq!(login.extra_fields[1].value, "user@example.com");
        assert_eq!(login.extra_fields[2].value, "s3cret");
        assert!(matches!(login.extra_fields[1].field_type, FieldType::Login));
        assert!(matches!(
            login.extra_fields[2].field_type,
            FieldType::Password
        ));

        let server = &imported.children[1];
        assert_eq!(server.name, "Server");
        assert_eq!(server.extra_fields[0].value, "SSH config");
        assert_eq!(server.extra_fields[1].value, "example.com");
        assert_eq!(server.children.len(), 1);
        assert_eq!(server.children[0].name, "Key");
    }

    #[test]
    fn test_import_csv() {
        let imported = import_csv(sample_csv()).unwrap();
        assert_eq!(imported.name, "Secrets");
        // The legacy description column migrates to a leading extra field.
        assert_eq!(imported.extra_fields[0].field_type.label(), "Description");
        assert_eq!(imported.extra_fields[0].value, "My vault");
        assert_eq!(imported.children.len(), 2);

        let login = &imported.children[0];
        assert_eq!(login.name, "Login");
        assert_eq!(login.extra_fields.len(), 3);
        assert_eq!(login.extra_fields[0].value, "My email login");
        assert_eq!(login.extra_fields[1].value, "user@example.com");

        let server = &imported.children[1];
        assert_eq!(server.name, "Server");
        assert_eq!(server.children.len(), 1);
        assert_eq!(server.children[0].name, "Key");
    }

    #[test]
    fn test_import_csv_current_header_without_description() {
        let data = "path,name,field_type,field_value\n\
                    Root,Root,, \n\
                    Root > Child,Child,Login,user";
        let node = import_csv(data).unwrap();
        assert_eq!(node.name, "Root");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].extra_fields.len(), 1);
        assert_eq!(node.children[0].extra_fields[0].value, "user");
        assert_eq!(node.children[0].extra_fields[0].field_type.label(), "Login");
    }

    #[test]
    fn test_import_csv_invalid_short_row() {
        let data = "path,name,field_type,field_value\nonlypath";
        let result = import_csv(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_csv_empty() {
        let result = import_csv("");
        assert!(result.is_err());
    }

    #[test]
    fn a_header_only_csv_is_rejected_as_having_no_rows() {
        let error = import_csv("path,name,field_type,field_value").unwrap_err();
        assert!(matches!(
            error,
            FileHandlingError::ImportSyntax(ImportSyntax::NoCsvDataRows)
        ));
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
        assert_eq!(node.extra_fields[0].value, "x < y");
        assert_eq!(node.extra_fields[1].value, "a\"b");
    }

    #[test]
    fn test_import_xml_without_description_has_no_field() {
        let xml = r#"<?xml version="1.0"?>
<node name="NoDesc">
  <extra_fields>
    <field type="Login" value="u"/>
  </extra_fields>
</node>"#;
        let node = import_xml(xml).unwrap();
        assert_eq!(node.extra_fields.len(), 1);
        assert_eq!(node.extra_fields[0].value, "u");
    }

    #[test]
    fn test_import_xml_empty_description_has_no_field() {
        let xml = r#"<?xml version="1.0"?>
<node name="EmptyDesc" description="">
  <extra_fields>
  </extra_fields>
</node>"#;
        let node = import_xml(xml).unwrap();
        assert!(node.extra_fields.is_empty());
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
        assert_eq!(node.extra_fields.len(), 1); // migrated description
        assert_eq!(node.extra_fields[0].value, "no children");
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
        let attrs = extract_xml_attrs(r#"name="Root" description="hello world""#).unwrap();
        assert_eq!(attrs.len(), 2);
        assert_eq!(attrs[0].0, "name");
        assert_eq!(attrs[0].1, "Root");
        assert_eq!(attrs[1].0, "description");
        assert_eq!(attrs[1].1, "hello world");
    }

    #[test]
    fn an_attribute_without_a_value_is_rejected() {
        assert!(matches!(
            extract_xml_attrs(r#"name="Root" broken"#),
            Err(FileHandlingError::ImportSyntax(
                ImportSyntax::MalformedAttribute
            ))
        ));
    }

    #[test]
    fn an_attribute_with_an_unterminated_quote_is_rejected() {
        assert!(matches!(
            extract_xml_attrs(r#"name="Root"#),
            Err(FileHandlingError::ImportSyntax(
                ImportSyntax::MalformedAttribute
            ))
        ));
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
        assert_eq!(node.children[0].extra_fields.len(), 1); // migrated description
        assert_eq!(node.children[0].extra_fields[0].value, "leaf");
    }

    #[test]
    fn test_csv_with_values_all_empty() {
        let data = "path,name,description,field_type,field_value\n\
                     Root,Root,desc,,\n\
                     Root,Root,desc,,,\n";
        let node = import_csv(data).unwrap();
        assert_eq!(node.name, "Root");
        assert_eq!(node.extra_fields.len(), 1); // migrated description
        assert_eq!(node.extra_fields[0].value, "desc");
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
        assert_eq!(node.children[0].extra_fields[0].value, "A");
        assert_eq!(node.children[1].name, "Beta");
    }

    #[test]
    fn test_csv_escape_roundtrip() {
        let data = r#"path,name,description,field_type,field_value
"Root","Name with, comma","Desc with "" quote",,
"Root > Child","Child","desc",Login,"va,lue"""#;
        let node = import_csv(data).unwrap();
        assert_eq!(node.name, "Name with, comma");
        assert_eq!(node.extra_fields[0].value, "Desc with \" quote");
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].extra_fields.len(), 2);
        assert_eq!(node.children[0].extra_fields[0].value, "desc");
        assert_eq!(node.children[0].extra_fields[1].value, "va,lue\"");
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
        assert_eq!(original.extra_fields.len(), imported.extra_fields.len());
        for (a, b) in original
            .extra_fields
            .iter()
            .zip(imported.extra_fields.iter())
        {
            assert_eq!(a.value, b.value);
        }
        assert_eq!(original.children.len(), imported.children.len());
        for (a, b) in original.children.iter().zip(imported.children.iter()) {
            assert_nodes_match(a, b);
        }
    }

    #[test]
    fn test_xml_roundtrip_simple() {
        let mut original = Node::new("Test");
        original.extra_fields = vec![ExtraField::description("simple roundtrip")];
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
    }

    #[test]
    fn test_xml_roundtrip_nested() {
        let mut child = Node::new("Login");
        child.extra_fields = vec![
            ExtraField::description("My login"),
            ExtraField {
                field_type: FieldType::Login,
                value: "user".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "pass".to_string(),
            },
        ];
        let original = Node::with_children("Root", vec![child]);
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
        assert_eq!(imported.children[0].extra_fields.len(), 3);
        assert_eq!(imported.children[0].extra_fields[1].value, "user");
    }

    #[test]
    fn test_xml_roundtrip_deeply_nested() {
        let leaf = Node::new("Leaf");
        let mid = Node::with_children("Middle", vec![leaf]);
        let mut original = Node::with_children("Root", vec![mid]);
        original.extra_fields = vec![ExtraField::description("root")];
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
        assert_eq!(imported.children[0].children[0].name, "Leaf");
    }

    #[test]
    fn test_xml_roundtrip_special_chars() {
        let mut original = Node::new("A & B <C>");
        original.extra_fields = vec![
            ExtraField::description(r#"description "with" quotes'apos"#),
            ExtraField {
                field_type: FieldType::Custom("<tag>".to_string()),
                value: "a&b".to_string(),
            },
        ];
        let imported = roundtrip_node(&original);
        assert_nodes_match(&original, &imported);
    }

    #[test]
    fn test_csv_roundtrip() {
        let mut child = Node::new("Login");
        child.extra_fields = vec![
            ExtraField::description("My login"),
            ExtraField {
                field_type: FieldType::Login,
                value: "user".to_string(),
            },
            ExtraField {
                field_type: FieldType::Password,
                value: "pass".to_string(),
            },
        ];
        let original = Node::with_children("Root", vec![child]);

        let csv_path = format!("/tmp/roundtrip_csv_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
    }

    #[test]
    fn test_csv_roundtrip_multiple_extra_fields() {
        let mut child = Node::new("Entry");
        child.extra_fields = vec![
            ExtraField::description("Login creds"),
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
        let original = Node::with_children("Secrets", vec![child]);

        let csv_path = format!("/tmp/roundtrip_csv2_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
        assert_eq!(imported.children[0].extra_fields.len(), 4);
        assert_eq!(imported.children[0].extra_fields[1].value, "admin");
    }

    #[test]
    fn test_json_roundtrip_via_import_tree() {
        let mut original =
            Node::with_children("Root", vec![Node::new("Child1"), Node::new("Child2")]);
        original.extra_fields = vec![ExtraField::description("desc")];
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
            FileHandlingError::UnsupportedImportFormat(_)
        ));
    }

    // ── Round-trip and hardening tests ──

    #[test]
    fn unescape_decodes_each_entity_exactly_once() {
        // `&amp;lt;` is the correct escape of a literal `&lt;`: decoding the
        // entities sequentially would turn it into `<`.
        assert_eq!(unescape_xml("&amp;lt;"), "&lt;");
        assert_eq!(unescape_xml("&amp;amp;"), "&amp;");
        assert_eq!(unescape_xml("&amp;"), "&");
        assert_eq!(unescape_xml("a &lt; b &amp; c &gt; d"), "a < b & c > d");
        // An unknown entity is preserved verbatim.
        assert_eq!(unescape_xml("&nbsp;"), "&nbsp;");
    }

    #[test]
    fn xml_roundtrip_preserves_a_literal_entity() {
        let mut original = Node::new("Root");
        original.extra_fields = vec![ExtraField {
            field_type: FieldType::Custom("Note".to_string()),
            value: "literal &lt; tag &amp; more".to_string(),
        }];

        let imported = roundtrip_node(&original);

        assert_nodes_match(&original, &imported);
        assert_eq!(
            imported.extra_fields[0].value,
            "literal &lt; tag &amp; more"
        );
    }

    #[test]
    fn csv_roundtrip_preserves_an_embedded_newline() {
        let mut child = Node::new("Entry");
        child.extra_fields = vec![ExtraField {
            field_type: FieldType::Password,
            value: "first line\nsecond line".to_string(),
        }];
        let original = Node::with_children("Root", vec![child]);

        let csv_path = format!("/tmp/roundtrip_csv_nl_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
        assert_eq!(
            imported.children[0].extra_fields[0].value,
            "first line\nsecond line"
        );
    }

    #[test]
    fn csv_roundtrip_preserves_a_bare_carriage_return() {
        let mut child = Node::new("Entry");
        child.extra_fields = vec![ExtraField {
            field_type: FieldType::Password,
            value: "before\rafter".to_string(),
        }];
        let original = Node::with_children("Root", vec![child]);

        let csv_path = format!("/tmp/roundtrip_csv_cr_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
        assert_eq!(imported.children[0].extra_fields[0].value, "before\rafter");
    }

    #[test]
    fn import_csv_accepts_a_reordered_and_quoted_header() {
        let data = "\"name\",\"path\",\"field_value\",\"field_type\"\n\
                    Root,Root,val,Login";
        let node = import_csv(data).unwrap();

        assert_eq!(node.name, "Root");
        assert_eq!(node.extra_fields[0].value, "val");
        assert_eq!(node.extra_fields[0].field_type.label(), "Login");
    }

    #[test]
    fn a_short_csv_row_is_reported_by_line_without_its_contents() {
        let data = "path,name,field_type,field_value\n\
                    Root,Root,Login,user\n\
                    onlypath";
        let error = import_csv(data).unwrap_err();

        assert!(matches!(
            error,
            FileHandlingError::CsvRowTooFewFields { line: 3 }
        ));
        // The row's contents (a decrypted password) must never be echoed.
        assert!(!error.to_string().contains("onlypath"));
        assert_eq!(error.to_string(), "import: CSV row 3 has too few fields");
    }

    fn nested_xml(depth: usize) -> String {
        let mut xml = String::from("<?xml version=\"1.0\"?>\n");
        for i in 0..depth.saturating_sub(1) {
            xml.push_str(&format!(
                "<node name=\"n{i}\"><extra_fields></extra_fields><children>"
            ));
        }
        xml.push_str("<node name=\"leaf\"><extra_fields></extra_fields></node>");
        for _ in 0..depth.saturating_sub(1) {
            xml.push_str("</children></node>");
        }
        xml
    }

    #[test]
    fn xml_nesting_beyond_the_limit_is_rejected() {
        assert!(import_xml(&nested_xml(10)).is_ok());

        let error = import_xml(&nested_xml(MAX_XML_DEPTH + 5)).unwrap_err();
        assert!(matches!(
            error,
            FileHandlingError::ImportSyntax(ImportSyntax::XmlNestingTooDeep)
        ));
    }

    #[test]
    fn a_csv_path_beyond_the_limit_is_rejected() {
        let deep_path = (0..MAX_CSV_PATH_DEPTH + 5)
            .map(|i| format!("s{i}"))
            .collect::<Vec<_>>()
            .join(" > ");
        let data = format!("path,name,field_type,field_value\n{deep_path},Leaf,,");

        let error = import_csv(&data).unwrap_err();
        assert!(matches!(
            error,
            FileHandlingError::ImportSyntax(ImportSyntax::CsvPathTooDeep)
        ));
    }

    #[test]
    fn xml_parse_errors_name_the_problem() {
        let cases = [
            (
                "<?xml version=\"1.0\"",
                ImportSyntax::UnterminatedXmlDeclaration,
            ),
            ("<node name=\"X\">", ImportSyntax::ExpectedExtraFieldsStart),
            (
                "<node ><extra_fields></extra_fields></node>",
                ImportSyntax::MissingNodeName,
            ),
            (
                "<node name=\"X\"><extra_fields></extra_fields>",
                ImportSyntax::ExpectedNodeEnd,
            ),
        ];

        for (xml, expected) in cases {
            let error = import_xml(xml).unwrap_err();
            assert!(
                matches!(error, FileHandlingError::ImportSyntax(reason) if reason == expected),
                "{xml:?} should report {expected:?}, got {error:?}"
            );
        }
    }

    #[test]
    fn import_tree_enforces_the_size_cap() {
        let path = format!("/tmp/import_too_large_{}.json", uuid::Uuid::new_v4());
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_FILE_SIZE + 1).unwrap();
        drop(file);

        let result = import_tree(&path);
        let _ = std::fs::remove_file(&path);

        assert!(matches!(
            result,
            Err(FileHandlingError::FileTooLarge { .. })
        ));
    }
}
