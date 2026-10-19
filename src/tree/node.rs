use std::borrow::Cow;

use serde::{Deserialize, Deserializer, Serialize};
use uuid::Uuid;

use crate::theming::texts;

/// The type of an extra field attached to a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    Login,
    Password,
    Custom(String),
}

impl FieldType {
    /// The display label for this field type.
    ///
    /// Only the `Custom` variant allocates; the others return a `'static` str.
    pub fn label(&self) -> Cow<'_, str> {
        match self {
            FieldType::Login => Cow::Borrowed(texts::FIELD_LOGIN),
            FieldType::Password => Cow::Borrowed(texts::FIELD_PASSWORD),
            FieldType::Custom(name) => Cow::Borrowed(name),
        }
    }

    /// The wire label used by import/export formats.
    ///
    /// Custom fields are prefixed with `custom:` so they round-trip
    /// unambiguously and cannot be confused with the built-in `Login` and
    /// `Password` types (e.g. a custom field literally named `Login`).
    pub fn wire_label(&self) -> Cow<'_, str> {
        match self {
            FieldType::Login => Cow::Borrowed(texts::FIELD_LOGIN),
            FieldType::Password => Cow::Borrowed(texts::FIELD_PASSWORD),
            FieldType::Custom(name) => Cow::Owned(format!("custom:{name}")),
        }
    }

    /// Parses a wire label back into a [`FieldType`].
    pub fn from_wire_label(s: &str) -> Self {
        match s {
            texts::FIELD_LOGIN => FieldType::Login,
            texts::FIELD_PASSWORD => FieldType::Password,
            other => match other.strip_prefix("custom:") {
                Some(name) => FieldType::Custom(name.to_string()),
                None => FieldType::Custom(other.to_string()),
            },
        }
    }

    /// Returns the value string to display.
    ///
    /// Password fields are masked as `*****`; all other types show the raw value.
    pub fn display_value<'a>(&self, value: &'a str) -> Cow<'a, str> {
        match self {
            FieldType::Password => Cow::Borrowed(texts::PASSWORD_MASK),
            _ => Cow::Borrowed(value),
        }
    }
}

/// A single extra field (login, password, or custom) on a node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraField {
    pub field_type: FieldType,
    pub value: String,
}

impl ExtraField {
    /// Creates a Description field: a `Custom` field labelled "Description".
    ///
    /// Node descriptions are ordinary extra fields; this is the canonical way
    /// to build one.
    pub fn description(value: impl Into<String>) -> Self {
        Self {
            field_type: FieldType::Custom(texts::FIELD_ITEM_DESC.to_string()),
            value: value.into(),
        }
    }
}

/// Prepends a legacy `description` (when non-empty) to `extra_fields` as a
/// Description field.
///
/// Older vaults and interchange files stored the description in a dedicated
/// slot instead of an extra field. Importers call this so those descriptions
/// survive the move to the uniform extra-field model. An empty or absent
/// description produces no field.
pub fn migrate_legacy_description(description: Option<&str>, extra_fields: &mut Vec<ExtraField>) {
    if let Some(desc) = description
        && !desc.is_empty()
    {
        extra_fields.insert(0, ExtraField::description(desc));
    }
}

/// A node in the tree with a unique identifier, display name, optional extra
/// fields, and optional children.
///
/// The node's description is an ordinary [`ExtraField`]; see
/// [`ExtraField::description`]. Deserialization migrates a legacy top-level
/// `description` key into an extra field for backward compatibility.
#[derive(Debug, Clone, Serialize)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub extra_fields: Vec<ExtraField>,
    pub children: Vec<Node>,
}

/// Helper mirroring [`Node`] with the legacy `description` key for
/// deserialization; not part of the public model.
#[derive(Deserialize)]
struct NodeRepr {
    id: Uuid,
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    extra_fields: Vec<ExtraField>,
    #[serde(default)]
    children: Vec<Node>,
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let repr = NodeRepr::deserialize(deserializer)?;
        let mut extra_fields = repr.extra_fields;
        migrate_legacy_description(repr.description.as_deref(), &mut extra_fields);
        Ok(Node {
            id: repr.id,
            name: repr.name,
            extra_fields,
            children: repr.children,
        })
    }
}

impl Node {
    /// Creates a leaf node with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            extra_fields: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Creates a branch node with pre-populated children.
    pub fn with_children(name: impl Into<String>, children: Vec<Node>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            extra_fields: Vec::new(),
            children,
        }
    }

    /// Returns `true` if the node has no children (a leaf node).
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// Appends a child node to this node's children.
    pub fn add_child(&mut self, child: Node) {
        self.children.push(child);
    }

    /// Recursively finds a descendant node by [`Uuid`], returning a shared
    /// reference to it.
    pub fn find(&self, id: Uuid) -> Option<&Node> {
        if self.id == id {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.find(id) {
                return Some(found);
            }
        }
        None
    }

    /// Recursively finds a descendant node by [`Uuid`], returning a mutable
    /// reference to it.
    pub fn find_mut(&mut self, id: Uuid) -> Option<&mut Node> {
        if self.id == id {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.find_mut(id) {
                return Some(found);
            }
        }
        None
    }

    /// Returns `true` if a node with the given [`Uuid`] exists anywhere in
    /// this subtree (including the root itself).
    pub fn contains(&self, id: Uuid) -> bool {
        if self.id == id {
            return true;
        }
        self.children.iter().any(|c| c.contains(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_json(description: Option<&str>, extra_fields: &str) -> String {
        let desc = match description {
            Some(d) => format!(r#""description":"{d}","#),
            None => String::new(),
        };
        format!(
            r#"{{"id":"00000000-0000-0000-0000-000000000000","name":"N",{desc}"extra_fields":[{extra_fields}],"children":[]}}"#
        )
    }

    #[test]
    fn legacy_description_migrates_to_leading_extra_field() {
        let node: Node = serde_json::from_str(&legacy_json(Some("hello"), "")).unwrap();
        assert_eq!(node.extra_fields.len(), 1);
        assert_eq!(node.extra_fields[0].value, "hello");
        match &node.extra_fields[0].field_type {
            FieldType::Custom(name) => assert_eq!(name, "Description"),
            other => panic!("expected Custom, got {other:?}"),
        }
    }

    #[test]
    fn legacy_empty_description_produces_no_field() {
        let node: Node = serde_json::from_str(&legacy_json(Some(""), "")).unwrap();
        assert!(node.extra_fields.is_empty());
    }

    #[test]
    fn absent_description_produces_no_field() {
        let node: Node = serde_json::from_str(&legacy_json(None, "")).unwrap();
        assert!(node.extra_fields.is_empty());
    }

    #[test]
    fn legacy_description_is_prepended_before_existing_fields() {
        let existing = r#"{"field_type":"Login","value":"u"}"#;
        let node: Node = serde_json::from_str(&legacy_json(Some("first"), existing)).unwrap();
        assert_eq!(node.extra_fields.len(), 2);
        assert_eq!(node.extra_fields[0].value, "first");
        assert_eq!(node.extra_fields[1].value, "u");
    }

    #[test]
    fn new_json_has_no_description_key() {
        let mut node = Node::new("N");
        node.extra_fields.push(ExtraField::description("d"));
        let json = serde_json::to_string(&node).unwrap();
        assert!(!json.contains("description"));
        let decoded: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.extra_fields.len(), 1);
        assert_eq!(decoded.extra_fields[0].value, "d");
    }
}
