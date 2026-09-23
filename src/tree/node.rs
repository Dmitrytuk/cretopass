use std::borrow::Cow;

use serde::{Deserialize, Serialize};
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

/// A node in the tree with a unique identifier, display name, description,
/// optional extra fields, and optional children.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub extra_fields: Vec<ExtraField>,
    pub children: Vec<Node>,
}

impl Node {
    /// Creates a leaf node with the given name and description.
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            extra_fields: Vec::new(),
            children: Vec::new(),
        }
    }

    /// Creates a branch node with pre-populated children.
    pub fn with_children(
        name: impl Into<String>,
        description: impl Into<String>,
        children: Vec<Node>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
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
