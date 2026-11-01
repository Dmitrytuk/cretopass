//! The flattened CSV interchange format: one module owns both directions.
//!
//! The writer ([`node_to_csv`]) and the reader ([`import_csv`]) live together so
//! the `path`-segment escaping, the row layout and the header contract cannot
//! drift apart between them.

use std::collections::HashMap;

use zeroize::Zeroizing;

use super::import::ParseError;
use crate::tree::node::{ExtraField, FieldType, Node, migrate_legacy_description};
use uuid::Uuid;

/// Maximum number of `>`-separated segments accepted in a CSV path, mirroring
/// the XML importer's depth bound.
const MAX_CSV_PATH_DEPTH: usize = 256;

// ── Writer ──

/// Serialises `node` to a flattened CSV document.
///
/// The result holds every secret in the tree in the clear, so it is returned in
/// a [`Zeroizing`] string that wipes it when the caller drops it.
pub(super) fn node_to_csv(node: &Node) -> Zeroizing<String> {
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

// ── Path segments ──

/// Encodes one node name as a single `path` segment of a flattened CSV export.
///
/// The path column joins segments with `" > "`, so a name that itself contains
/// `" > "` would otherwise be re-interpreted as nesting on import. Escaping `\`,
/// `#`, and `>` guarantees two properties the reader relies on:
///
/// - every `>` inside a segment is preceded by `\`, so `" > "` can only be the
///   real separator; and
/// - every `#` inside a segment is preceded by `\`, so an *unescaped* `#` marks
///   the occurrence suffix [`encode_path_segment`]'s caller appends for
///   same-named siblings (see [`decode_path_segment`]).
fn encode_path_segment(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '#' => out.push_str("\\#"),
            '>' => out.push_str("\\>"),
            _ => out.push(c),
        }
    }
    out
}

/// Decodes one `path` segment back to its display name.
///
/// Drops a trailing `#<n>` occurrence suffix (emitted by this module's writer
/// for the second and later same-named siblings) and then undoes the escaping
/// applied by [`encode_path_segment`]. The suffix is only recognised when the
/// `#` is unescaped and followed by digits, so foreign input is not truncated
/// arbitrarily.
///
/// One documented edge remains: an older file whose *intermediate* node name
/// ends in `#<digits>` (and has no row of its own) is decoded as the part before
/// the `#`. A node's own row is never affected, because its name comes from the
/// authoritative `name` column.
fn decode_path_segment(segment: &str) -> String {
    let (name, _) = split_occurrence_suffix(segment);
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('#') => out.push('#'),
            Some('>') => out.push('>'),
            // An escape we never emit (a legacy name's stray `\`), preserved.
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Splits `segment` at its first unescaped `#<digits>`, returning the encoded
/// name and the occurrence suffix (without the `#`) when one is present.
fn split_occurrence_suffix(segment: &str) -> (&str, Option<&str>) {
    let mut chars = segment.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => {
                // Skip the escaped character, whatever it is.
                chars.next();
            }
            '#' => {
                let suffix = &segment[i + 1..];
                if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) {
                    return (&segment[..i], Some(suffix));
                }
                // An unescaped `#` that is not an occurrence marker is part of
                // the name; our writer only ever appends one suffix, at the end.
                return (segment, None);
            }
            _ => {}
        }
    }
    (segment, None)
}

// ── Reader ──

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
    /// Returns [`ParseError::MissingCsvHeader`] when the header has no `path`
    /// or `name` column, i.e. the first record is not a recognisable header.
    fn from_header(header: &[String]) -> Result<Self, ParseError> {
        let find = |column: &str| header.iter().position(|c| c.trim() == column);
        Ok(Self {
            path: find("path").ok_or(ParseError::MissingCsvHeader)?,
            name: find("name").ok_or(ParseError::MissingCsvHeader)?,
            description: find("description"),
            field_type: find("field_type"),
            field_value: find("field_value"),
        })
    }
}

pub(super) fn import_csv(data: &str) -> Result<Node, ParseError> {
    let mut records = parse_csv_records(data);
    if records.is_empty() {
        return Err(ParseError::EmptyCsv);
    }

    let (_, header) = records.remove(0);
    let cols = CsvColumns::from_header(&header)?;
    if records.is_empty() {
        return Err(ParseError::NoCsvDataRows);
    }

    let get = |fields: &[String], idx: Option<usize>| -> Option<String> {
        idx.and_then(|i| fields.get(i)).cloned()
    };

    // Group the rows by path: the first row for a path carries its name and
    // description, and every row contributes an extra field. The `index` map
    // keeps this linear on a large file; `order` keeps first-seen order.
    let mut order: Vec<(String, PathData)> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (line, fields) in &records {
        if fields.iter().all(|field| field.trim().is_empty()) {
            continue;
        }
        if fields.len() <= cols.path || fields.len() <= cols.name {
            return Err(ParseError::CsvRowTooFewFields { line: *line });
        }
        let path = fields[cols.path].clone();
        let field_type = get(fields, cols.field_type).filter(|ft| !ft.is_empty());
        let field_value = get(fields, cols.field_value).unwrap_or_default();

        let idx = match index.get(&path).copied() {
            Some(idx) => idx,
            None => {
                let idx = order.len();
                index.insert(path.clone(), idx);
                order.push((
                    path,
                    PathData {
                        name: fields[cols.name].clone(),
                        description: get(fields, cols.description).filter(|d| !d.is_empty()),
                        extra_fields: Vec::new(),
                    },
                ));
                idx
            }
        };

        if let Some(ft) = field_type {
            order[idx].1.extra_fields.push(ExtraField {
                field_type: FieldType::from_wire_label(&ft),
                value: field_value,
            });
        }
    }

    // Build tree from paths
    // Each path is like "Root > Category > Item"
    build_tree_from_paths(order)
}

/// Builds the tree from `paths`, one entry per CSV path, in first-seen order.
///
/// A path is `"Root > Category > Item"`; each segment is a node, created on
/// first sight and reused after. A `HashMap` per level keys children by their
/// segment, so a row costs one lookup per segment rather than a scan of its
/// siblings (see [`encode_path_segment`] for how a segment that contains `>` is
/// kept unambiguous).
fn build_tree_from_paths(paths: Vec<(String, PathData)>) -> Result<Node, ParseError> {
    struct TreeNode {
        name: String,
        description: Option<String>,
        extra_fields: Vec<ExtraField>,
        children: Vec<TreeNode>,
        /// Child position by segment, so a repeated segment is found in O(1).
        child_index: HashMap<String, usize>,
    }

    /// Descends `segments`, creating nodes as needed, and fills the last one
    /// with `data`.
    fn insert_path(
        children: &mut Vec<TreeNode>,
        index: &mut HashMap<String, usize>,
        segments: &[&str],
        data: PathData,
    ) {
        let segment = segments[0];
        let is_last = segments.len() == 1;

        let pos = match index.get(segment).copied() {
            Some(pos) => pos,
            None => {
                let pos = children.len();
                children.push(TreeNode {
                    // An intermediate node with no row of its own (a legacy
                    // file) decodes its name from the segment; a row sets its
                    // own name below, so a leaf needs no placeholder.
                    name: if is_last {
                        String::new()
                    } else {
                        decode_path_segment(segment)
                    },
                    description: None,
                    extra_fields: Vec::new(),
                    children: Vec::new(),
                    child_index: HashMap::new(),
                });
                index.insert(segment.to_string(), pos);
                pos
            }
        };

        if is_last {
            let node = &mut children[pos];
            node.name = data.name;
            node.description = data.description;
            node.extra_fields = data.extra_fields;
        } else {
            let node = &mut children[pos];
            insert_path(
                &mut node.children,
                &mut node.child_index,
                &segments[1..],
                data,
            );
        }
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
    let mut root_index: HashMap<String, usize> = HashMap::new();

    for (path_str, data) in paths {
        let segments: Vec<&str> = path_str.split(" > ").collect();
        if segments.len() > MAX_CSV_PATH_DEPTH {
            return Err(ParseError::CsvPathTooDeep);
        }
        insert_path(&mut roots, &mut root_index, &segments, data);
    }

    if roots.is_empty() {
        return Err(ParseError::NoTree);
    }

    if roots.len() == 1 {
        let root = roots.pop().expect("just checked there is exactly one root");
        Ok(into_node(root))
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_csv() -> &'static str {
        "path,name,description,field_type,field_value\n\
         Secrets,Secrets,My vault,Note,top level note\n\
         Secrets > Login,Login,My email login,Login,user@example.com\n\
         Secrets > Login,Login,My email login,Password,s3cret\n\
         Secrets > Server,Server,SSH config,Custom:Host,example.com\n\
         Secrets > Server > Key,Key,private key path,Password,encrypted-key"
    }

    /// Parses a single CSV line into its fields, for the tests below.
    fn parse_csv_line(line: &str) -> Vec<String> {
        parse_csv_records(line)
            .into_iter()
            .next()
            .map(|(_, fields)| fields)
            .unwrap_or_default()
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
        assert_eq!(error, ParseError::NoCsvDataRows);
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
    fn import_csv_groups_rows_that_revisit_a_path() {
        // A path may reappear after another path; its rows must merge into one
        // node, and the first row's name/description must win.
        let data = "path,name,field_type,field_value\n\
                     Root,A,Login,alice\n\
                     Other,B,Login,bob\n\
                     Root,A,Password,secret\n";
        let node = import_csv(data).unwrap();

        assert_eq!(node.name, "Imported");
        assert_eq!(node.children.len(), 2);
        let root = &node.children[0];
        assert_eq!(root.name, "A");
        assert_eq!(root.extra_fields.len(), 2);
        assert_eq!(root.extra_fields[0].value, "alice");
        assert_eq!(root.extra_fields[1].value, "secret");
    }

    #[test]
    fn import_csv_builds_many_sibling_paths() {
        // A scale smoke test for the linear grouping and child lookup; it does
        // not assert timing, only that a wide file imports correctly in one
        // pass (a quadratic pairing would still pass, just slowly).
        let mut data = String::from("path,name,field_type,field_value\n");
        for i in 0..5_000 {
            data.push_str(&format!("Root > n{i},n{i},Login,user{i}\n"));
        }

        let node = import_csv(&data).unwrap();

        assert_eq!(node.name, "Root");
        assert_eq!(node.children.len(), 5_000);
        assert_eq!(node.children[4_999].name, "n4999");
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

        assert_eq!(error, ParseError::CsvRowTooFewFields { line: 3 });
        // The row's contents (a decrypted password) must never be echoed.
        assert!(!error.to_string().contains("onlypath"));
        assert_eq!(error.to_string(), "CSV row 3 has too few fields");
    }

    #[test]
    fn a_csv_path_beyond_the_limit_is_rejected() {
        let deep_path = (0..MAX_CSV_PATH_DEPTH + 5)
            .map(|i| format!("s{i}"))
            .collect::<Vec<_>>()
            .join(" > ");
        let data = format!("path,name,field_type,field_value\n{deep_path},Leaf,,");

        let error = import_csv(&data).unwrap_err();
        assert_eq!(error, ParseError::CsvPathTooDeep);
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
    fn test_csv_escape() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("has,comma"), "\"has,comma\"");
        assert_eq!(csv_escape("has\"quote"), "\"has\"\"quote\"");
        assert_eq!(csv_escape("has\nnewline"), "\"has\nnewline\"");
        assert_eq!(csv_escape("has\rcarriage"), "\"has\rcarriage\"");
    }

    #[test]
    fn a_plain_name_round_trips_through_a_path_segment() {
        assert_eq!(encode_path_segment("Login"), "Login");
        assert_eq!(decode_path_segment(&encode_path_segment("Login")), "Login");
    }

    #[test]
    fn the_path_separator_inside_a_name_is_escaped() {
        assert_eq!(encode_path_segment("A > B"), "A \\> B");
        assert_eq!(decode_path_segment("A \\> B"), "A > B");
        // The encoding never produces a bare `" > "` inside a segment.
        assert!(!encode_path_segment("A > B").contains(" > "));
    }

    #[test]
    fn a_backslash_and_hash_round_trip() {
        for name in ["a\\b", "a#b", "a\\#b", "#2", "a > b#3"] {
            let encoded = encode_path_segment(name);
            assert_eq!(decode_path_segment(&encoded), name, "{encoded:?}");
        }
    }

    #[test]
    fn an_occurrence_suffix_is_dropped_but_a_literal_hash_is_kept() {
        // `#2` appended after the encoded name disambiguates a sibling...
        assert_eq!(decode_path_segment("Dup#2"), "Dup");
        // ...while a `#` that is part of the name stays escaped and survives.
        assert_eq!(decode_path_segment("Dup\\#2"), "Dup#2");
        assert_eq!(decode_path_segment("Dup\\#2#2"), "Dup#2");
    }

    #[test]
    fn an_unescaped_hash_not_followed_by_digits_is_kept() {
        assert_eq!(decode_path_segment("Tag#"), "Tag#");
        assert_eq!(decode_path_segment("Tag#extra"), "Tag#extra");
        // Documented legacy edge: `#<digits>` cannot be told from a suffix.
        assert_eq!(decode_path_segment("Tag#1"), "Tag");
    }
}
