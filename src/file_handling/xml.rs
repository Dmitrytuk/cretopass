//! The XML interchange format: one module owns both directions.
//!
//! The writer ([`node_to_xml`]) and the reader ([`import_xml`]) live together so
//! an escape change cannot drift out of step between them: `escape` and
//! `unescape` are the two halves of one contract, and their round trip is tested
//! here.

use zeroize::Zeroizing;

use super::import::ParseError;
use crate::tree::node::{ExtraField, FieldType, Node, migrate_legacy_description};
use uuid::Uuid;

/// Maximum XML nesting depth accepted during import, guarding against
/// stack overflow from maliciously deep files.
const MAX_DEPTH: usize = 256;

// ── Writer ──

/// Serialises `node` to an XML document.
///
/// The result holds every secret in the tree in the clear, so it is returned in
/// a [`Zeroizing`] string that wipes it when the caller drops it.
pub(super) fn node_to_xml(node: &Node) -> Zeroizing<String> {
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

/// Escapes the five XML predefined entities.
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ── Reader ──

/// Parses an XML document into a tree.
pub(super) fn import_xml(data: &str) -> Result<Node, ParseError> {
    let trimmed = data.trim();
    // Skip XML declaration if present
    let body = if trimmed.starts_with("<?xml") {
        let end = trimmed
            .find("?>")
            .ok_or(ParseError::UnterminatedXmlDeclaration)?;
        trimmed[end + 2..].trim()
    } else {
        trimmed
    };

    let (node, _rest) = parse_xml_node(body, 0)?;
    Ok(node)
}

fn parse_xml_node(input: &str, depth: usize) -> Result<(Node, &str), ParseError> {
    if depth > MAX_DEPTH {
        return Err(ParseError::XmlNestingTooDeep);
    }
    let input = input.trim_start();

    if !input.starts_with("<node") {
        return Err(ParseError::ExpectedNodeStart);
    }

    let tag_end = input.find('>').ok_or(ParseError::ExpectedNodeTagEnd)?;
    let attrs_str = &input[5..tag_end];

    let attrs = extract_xml_attrs(attrs_str)?;
    let name = attrs
        .iter()
        .find(|(k, _)| k == "name")
        .map(|(_, v)| unescape_xml(v))
        .ok_or(ParseError::MissingNodeName)?;
    let description = attrs
        .iter()
        .find(|(k, _)| k == "description")
        .map(|(_, v)| unescape_xml(v));

    let mut remaining = input[tag_end + 1..].trim_start();

    // Parse <extra_fields>
    let ef_open = "<extra_fields>";
    let ef_close = "</extra_fields>";
    if !remaining.starts_with(ef_open) {
        return Err(ParseError::ExpectedExtraFieldsStart);
    }
    remaining = &remaining[ef_open.len()..];

    let ef_end = remaining
        .find(ef_close)
        .ok_or(ParseError::ExpectedExtraFieldsEnd)?;
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
        return Err(ParseError::ExpectedNodeEnd);
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

fn extract_xml_attrs(s: &str) -> Result<Vec<(String, String)>, ParseError> {
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
        let eq_pos = remaining.find('=').ok_or(ParseError::MalformedAttribute)?;
        let key = remaining[..eq_pos].trim().to_string();
        remaining = remaining[eq_pos + 1..].trim_start();
        let after_quote = remaining
            .strip_prefix('"')
            .ok_or(ParseError::MalformedAttribute)?;
        let end = after_quote
            .find('"')
            .ok_or(ParseError::MalformedAttribute)?;
        attrs.push((key, after_quote[..end].to_string()));
        remaining = &after_quote[end + 1..];
    }
    Ok(attrs)
}

fn parse_xml_fields(input: &str) -> Result<Vec<ExtraField>, ParseError> {
    let mut fields = Vec::new();
    let mut remaining = input.trim();
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        if remaining.starts_with("</") || remaining.is_empty() {
            break;
        }
        if !remaining.starts_with("<field") {
            return Err(ParseError::ExpectedFieldStart);
        }
        remaining = &remaining["<field".len()..];

        let tag_end = remaining.find('>').ok_or(ParseError::ExpectedFieldTagEnd)?;
        let attrs_str = &remaining[..tag_end];
        remaining = &remaining[tag_end + 1..];

        let attrs = extract_xml_attrs(attrs_str)?;
        let field_type_label = attrs
            .iter()
            .find(|(k, _)| k == "type")
            .map(|(_, v)| unescape_xml(v))
            .ok_or(ParseError::MissingFieldType)?;
        let value = attrs
            .iter()
            .find(|(k, _)| k == "value")
            .map(|(_, v)| unescape_xml(v))
            .ok_or(ParseError::MissingFieldValue)?;

        let field_type = FieldType::from_wire_label(&field_type_label);

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

#[cfg(test)]
mod tests {
    use super::*;

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
            Err(ParseError::MalformedAttribute)
        ));
    }

    #[test]
    fn an_attribute_with_an_unterminated_quote_is_rejected() {
        assert!(matches!(
            extract_xml_attrs(r#"name="Root"#),
            Err(ParseError::MalformedAttribute)
        ));
    }

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

        let error = import_xml(&nested_xml(MAX_DEPTH + 5)).unwrap_err();
        assert_eq!(error, ParseError::XmlNestingTooDeep);
    }

    #[test]
    fn xml_parse_errors_name_the_problem() {
        let cases = [
            (
                "<?xml version=\"1.0\"",
                ParseError::UnterminatedXmlDeclaration,
            ),
            ("<node name=\"X\">", ParseError::ExpectedExtraFieldsStart),
            (
                "<node ><extra_fields></extra_fields></node>",
                ParseError::MissingNodeName,
            ),
            (
                "<node name=\"X\"><extra_fields></extra_fields>",
                ParseError::ExpectedNodeEnd,
            ),
        ];

        for (xml, expected) in cases {
            let error = import_xml(xml).unwrap_err();
            assert_eq!(error, expected, "{xml:?} should report {expected:?}");
        }
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
