//! Facade-level round-trip: `export_tree` then `import_tree`, through the
//! gateway alone, with no UI in the loop.
//!
//! This is the counterpart to the unit round-trips inside
//! [`import`](super::import): it proves the facade's re-exports are enough to
//! write a tree and read an equivalent one back for every format, and it pins
//! the two ambiguity cases the CSV path encoding exists for: a `" > "` inside a
//! name, and two same-named siblings.

use std::path::Path;

use crate::tree::node::{ExtraField, FieldType, Node};

use super::{ExportFormat, export_tree, import_tree};

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
/// [`assert_exhaustive`] makes the `match` exhaustive on the crate's
/// `ExportFormat` enum, so adding a variant fails to compile until it is listed
/// here: a format cannot silently escape this test.
fn all_formats() -> [ExportFormat; 3] {
    [ExportFormat::Json, ExportFormat::Xml, ExportFormat::Csv]
}

/// Fails to compile when a new [`ExportFormat`] variant escapes `all_formats`.
fn assert_exhaustive(format: ExportFormat) {
    match format {
        ExportFormat::Json | ExportFormat::Xml | ExportFormat::Csv => {}
    }
}

/// Asserts `actual` is structurally equal to `expected` (names, extra fields,
/// children), ignoring node ids, which are regenerated on import.
fn assert_same(expected: &Node, actual: &Node, format: ExportFormat) {
    assert_eq!(expected.name, actual.name, "{format:?}: node name");

    assert_eq!(
        expected.extra_fields.len(),
        actual.extra_fields.len(),
        "{format:?}: field count of {:?}",
        expected.name
    );
    for (expected_field, actual_field) in expected.extra_fields.iter().zip(&actual.extra_fields) {
        assert_eq!(
            expected_field.field_type.wire_label(),
            actual_field.field_type.wire_label(),
            "{format:?}: field label of {:?}",
            expected.name
        );
        assert_eq!(
            expected_field.value, actual_field.value,
            "{format:?}: field value of {:?}",
            expected.name
        );
    }

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
        assert_exhaustive(format);
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
