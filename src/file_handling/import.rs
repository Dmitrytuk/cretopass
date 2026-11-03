use std::fmt;
use std::path::Path;

use super::FileHandlingError;
use super::file_process::{MAX_FILE_SIZE, json_to_node, read_capped};
use super::format::Format;
use super::{csv, xml};
use crate::tree::node::Node;

/// The formats an imported document may use.
pub(crate) type ImportFormat = Format;

/// A structural problem found while parsing an imported document.
///
/// Every arm is a fixed message or a bare line number. No value read from the
/// imported file is ever carried here, so an imported secret can never reach a
/// user through an error's `Display`. This is the import layer's own error; the
/// gateway composes it into
/// [`FileHandlingError::Import`].
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    /// An XML declaration (`<?xml … ?>`) was never closed.
    UnterminatedXmlDeclaration,
    /// The XML document nests deeper than the import limit.
    XmlNestingTooDeep,
    /// A CSV path has more `>`-separated segments than the import limit.
    CsvPathTooDeep,
    /// A `<node>` element was expected.
    ExpectedNodeStart,
    /// A `<node …>` tag was never closed with `>`.
    ExpectedNodeTagEnd,
    /// A node carried no `name` attribute.
    MissingNodeName,
    /// An `<extra_fields>` element was expected.
    ExpectedExtraFieldsStart,
    /// An `</extra_fields>` element was expected.
    ExpectedExtraFieldsEnd,
    /// A `</node>` element was expected.
    ExpectedNodeEnd,
    /// A `<field>` element was expected.
    ExpectedFieldStart,
    /// A `<field …>` tag was never closed with `>`.
    ExpectedFieldTagEnd,
    /// A field carried no `type` attribute.
    MissingFieldType,
    /// A field carried no `value` attribute.
    MissingFieldValue,
    /// An attribute was missing its `=` or its closing quote.
    MalformedAttribute,
    /// The CSV document was empty.
    EmptyCsv,
    /// The CSV document had no header row.
    MissingCsvHeader,
    /// The CSV document had a header but no data rows.
    NoCsvDataRows,
    /// The CSV document produced no tree.
    NoTree,
    /// The JSON document was not a valid node.
    InvalidJson,
    /// A CSV data row (1-based `line`, counting the header) had fewer fields
    /// than the header requires.
    CsvRowTooFewFields {
        /// The 1-based physical line the row starts on.
        line: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnterminatedXmlDeclaration => f.write_str("unterminated XML declaration"),
            Self::XmlNestingTooDeep => f.write_str("XML nesting too deep"),
            Self::CsvPathTooDeep => f.write_str("CSV path is too deeply nested"),
            Self::ExpectedNodeStart => f.write_str("expected <node"),
            Self::ExpectedNodeTagEnd => f.write_str("expected > in <node"),
            Self::MissingNodeName => f.write_str("missing name attribute"),
            Self::ExpectedExtraFieldsStart => f.write_str("expected <extra_fields>"),
            Self::ExpectedExtraFieldsEnd => f.write_str("expected </extra_fields>"),
            Self::ExpectedNodeEnd => f.write_str("expected </node>"),
            Self::ExpectedFieldStart => f.write_str("expected <field"),
            Self::ExpectedFieldTagEnd => f.write_str("expected > in <field>"),
            Self::MissingFieldType => f.write_str("missing type in <field>"),
            Self::MissingFieldValue => f.write_str("missing value in <field>"),
            Self::MalformedAttribute => f.write_str("malformed attribute"),
            Self::EmptyCsv => f.write_str("empty CSV file"),
            Self::MissingCsvHeader => f.write_str("CSV file is missing its header row"),
            Self::NoCsvDataRows => f.write_str("CSV file has no data rows"),
            Self::NoTree => f.write_str("no data to import"),
            Self::InvalidJson => f.write_str("not a valid JSON node"),
            Self::CsvRowTooFewFields { line } => write!(f, "CSV row {line} has too few fields"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Reads `path` and builds a tree from it, choosing the parser by extension.
///
/// # Errors
///
/// Returns [`FileHandlingError::UnsupportedImportFormat`] for an extension
/// this module cannot parse, [`FileHandlingError::FileTooLarge`] when the file
/// exceeds the shared size cap, [`FileHandlingError::Io`] when it cannot be
/// read, and [`FileHandlingError::Import`] (a [`ParseError`]) when the document
/// is malformed.
pub fn import_tree(path: impl AsRef<Path>) -> Result<Node, FileHandlingError> {
    let path = path.as_ref();
    let format = ImportFormat::from_path(path)
        .ok_or_else(|| FileHandlingError::UnsupportedImportFormat(path.to_path_buf()))?;

    let bytes = read_capped(path, MAX_FILE_SIZE)?;
    let data = String::from_utf8(bytes).map_err(|e| {
        FileHandlingError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    })?;
    // A byte-order mark is what some editors (Excel, Google Sheets for "CSV
    // UTF-8") put in front of a UTF-8 file. It is not content: drop one before
    // dispatching, or every format's first token ("{", "<?xml", the CSV header)
    // fails to match.
    let data = data.strip_prefix('\u{feff}').unwrap_or(&data);

    // A malformed import is a syntax problem, not a damaged vault: report it in
    // the import vocabulary so the UI does not tell the user that the file they
    // are reading from is corrupt. The serde detail is dropped because it can
    // quote the document that failed to parse.
    let node = match format {
        Format::Json => json_to_node(data).map_err(|_| ParseError::InvalidJson)?,
        Format::Xml => xml::import_xml(data)?,
        Format::Csv => csv::import_csv(data)?,
    };
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_handling::{ExportFormat, export_tree};
    use crate::tree::node::{ExtraField, FieldType};

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
    fn import_tree_json_roundtrip() {
        let original = sample_node();
        let json = serde_json::to_string_pretty(&original).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("roundtrip.json");
        std::fs::write(&path, &json).unwrap();

        let imported = import_tree(&path).unwrap();
        // IDs are regenerated, so compare structure
        assert_eq!(imported.name, original.name);
        assert_eq!(imported.extra_fields.len(), original.extra_fields.len());
        assert_eq!(imported.children.len(), original.children.len());
        // Check extra field values
        assert_eq!(imported.extra_fields[0].value, "My vault");
        assert_eq!(imported.extra_fields[1].value, "top level note");
    }

    #[test]
    fn test_import_format_from_path() {
        let path = std::path::Path::new;
        assert_eq!(
            ImportFormat::from_path(path("file.json")),
            Some(ImportFormat::Json)
        );
        assert_eq!(
            ImportFormat::from_path(path("file.xml")),
            Some(ImportFormat::Xml)
        );
        assert_eq!(
            ImportFormat::from_path(path("file.csv")),
            Some(ImportFormat::Csv)
        );
        assert_eq!(ImportFormat::from_path(path("file.age")), None);
        assert_eq!(ImportFormat::from_path(path("file")), None);
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
    fn a_leading_utf8_bom_does_not_break_any_import() {
        let dir = tempfile::tempdir().unwrap();
        let samples = [
            ("json", serde_json::to_string(&sample_node()).unwrap()),
            ("xml", sample_xml().to_string()),
            ("csv", sample_csv().to_string()),
        ];

        for (ext, body) in samples {
            let path = dir.path().join(format!("bom.{ext}"));
            std::fs::write(&path, format!("\u{feff}{body}")).unwrap();

            let imported = import_tree(&path)
                .unwrap_or_else(|error| panic!("a {ext} file with a BOM must import: {error}"));
            assert_eq!(imported.name, "Secrets", "{ext}");
            if ext == "json" {
                // The JSON sample is a whole tree: compare it in full.
                assert_nodes_match(&sample_node(), &imported);
            }
        }
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
            // The wire label must survive too: a regression in `custom:` handling
            // would otherwise pass every round-trip assertion. `wire_label`
            // distinguishes `Login` from `Custom("Login")`, which `label` cannot.
            assert_eq!(a.field_type.wire_label(), b.field_type.wire_label());
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

    #[test]
    fn a_malformed_json_import_is_a_syntax_error_not_a_damaged_vault() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.json");
        std::fs::write(&path, "{ not json").unwrap();

        let error = import_tree(&path).unwrap_err();

        assert!(
            matches!(error, FileHandlingError::Import(ParseError::InvalidJson)),
            "a bad import is a syntax error, not a damaged secret: {error:?}"
        );
    }

    // ── Round-trip and hardening tests ──

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

    /// A name that contains the `" > "` path separator must not be mistaken for
    /// nesting on import (Blocker B2, case A).
    #[test]
    fn csv_roundtrip_preserves_a_name_containing_the_path_separator() {
        let mut original = Node::with_children("Root > X", vec![Node::new("A > B")]);
        original.extra_fields = vec![ExtraField::description("root > desc")];

        let csv_path = format!("/tmp/roundtrip_csv_sep_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
        assert_eq!(imported.name, "Root > X");
        assert_eq!(imported.children.len(), 1);
        assert_eq!(imported.children[0].name, "A > B");
    }

    /// Two siblings that share a name must stay two nodes, not collapse into one
    /// carrying both field sets (Blocker B2, case B).
    #[test]
    fn csv_roundtrip_keeps_same_named_siblings_apart() {
        let mut first = Node::new("Dup");
        first.extra_fields = vec![ExtraField {
            field_type: FieldType::Password,
            value: "first".to_string(),
        }];
        let mut second = Node::new("Dup");
        second.extra_fields = vec![ExtraField {
            field_type: FieldType::Password,
            value: "second".to_string(),
        }];
        let original = Node::with_children("Root", vec![first, second]);

        let csv_path = format!("/tmp/roundtrip_csv_dup_{}.csv", uuid::Uuid::new_v4());
        export_tree(&original, &csv_path, ExportFormat::Csv).unwrap();
        let imported = import_tree(&csv_path).unwrap();
        let _ = std::fs::remove_file(&csv_path);

        assert_nodes_match(&original, &imported);
        assert_eq!(
            imported.children.len(),
            2,
            "same-named siblings must not be merged"
        );
        assert_eq!(imported.children[0].extra_fields[0].value, "first");
        assert_eq!(imported.children[1].extra_fields[0].value, "second");
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
