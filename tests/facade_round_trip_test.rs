//! Facade-level round-trip: `export_tree` then `import_tree`, through
//! `file_handling` alone, with no UI in the loop.
//!
//! This is the integration counterpart to the unit round-trips inside
//! `file_handling::import`. It proves the gateway's public seam is enough to
//! write a tree and read an equivalent one back for every format, and it pins
//! the two ambiguity cases the CSV path encoding exists for: a `" > "` inside a
//! name, and two same-named siblings.

use std::path::Path;

use cretopass::file_handling::{ExportFormat, export_tree, import_tree};
use cretopass::{ExtraField, FieldType, Node};

fn custom(label: &str, value: &str) -> ExtraField {
    ExtraField {
        field_type: FieldType::Custom(label.to_string()),
        value: value.to_string(),
    }
}

/// A tree exercising every field kind plus the CSV-ambiguous names.
fn sample() -> Node {
    let mut login = Node::with_children("Login", vec![Node::new("Nested")]);
    login.extra_fields = vec![
        ExtraField {
            field_type: FieldType::Login,
            value: "user@example.com".to_string(),
        },
        ExtraField {
            field_type: FieldType::Password,
            // A comma, a quote and a newline: the CSV writer must quote it.
            value: "p,a\"ss\nword".to_string(),
        },
    ];

    let mut leaf = Node::new("A > B");
    leaf.extra_fields = vec![custom("Host", "example.com")];

    // Two same-named siblings with distinct values: the round-trip must keep
    // them apart *and* route each value to the right twin.
    let mut first_dup = Node::new("Dup");
    first_dup.extra_fields = vec![custom("Slot", "first")];
    let mut second_dup = Node::new("Dup");
    second_dup.extra_fields = vec![custom("Slot", "second")];

    let mut root = Node::with_children("Root > Vault", vec![login, first_dup, second_dup, leaf]);
    root.extra_fields = vec![custom("Note", "top & <level>")];
    root
}

/// Every export format.
///
/// The `match` is exhaustive on the crate's `Format` enum, so adding a variant
/// fails to compile until it is listed here: a format cannot silently escape
/// this test.
fn all_formats() -> Vec<ExportFormat> {
    let formats = [ExportFormat::Json, ExportFormat::Xml, ExportFormat::Csv];
    for format in formats {
        match format {
            ExportFormat::Json | ExportFormat::Xml | ExportFormat::Csv => {}
        }
    }
    formats.to_vec()
}

/// Asserts `actual` is structurally equal to `expected` (names, extra fields,
/// children), ignoring node ids, which are regenerated on import.
fn assert_same(expected: &Node, actual: &Node, format: ExportFormat) {
    assert_eq!(expected.name, actual.name, "{format:?}: node name");

    let fields = |node: &Node| {
        node.extra_fields
            .iter()
            .map(|f| (f.field_type.wire_label().into_owned(), f.value.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        fields(expected),
        fields(actual),
        "{format:?}: fields of {:?}",
        expected.name
    );

    assert_eq!(
        expected.children.len(),
        actual.children.len(),
        "{format:?}: children of {:?}",
        expected.name
    );
    for (a, b) in expected.children.iter().zip(&actual.children) {
        assert_same(a, b, format);
    }
}

#[test]
fn every_format_round_trips_through_the_gateway() {
    let dir = tempfile::tempdir().unwrap();
    let original = sample();

    for format in all_formats() {
        let base = dir.path().join("roundtrip").to_string_lossy().to_string();
        let written = export_tree(&original, &base, format).unwrap();
        assert!(
            Path::new(&written).exists(),
            "{format:?}: export reported {written} but wrote no file"
        );

        let reimported =
            import_tree(&written).unwrap_or_else(|e| panic!("{format:?}: import failed: {e}"));
        assert_same(&original, &reimported, format);
    }
}
