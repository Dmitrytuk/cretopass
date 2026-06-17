use uuid::Uuid;

/// The type of an extra field attached to a node.
#[derive(Debug, Clone)]
pub enum FieldType {
    Login,
    Password,
    Custom(String),
}

impl FieldType {
    /// The display label for this field type.
    pub fn label(&self) -> String {
        match self {
            FieldType::Login => "Login".to_string(),
            FieldType::Password => "Password".to_string(),
            FieldType::Custom(name) => name.clone(),
        }
    }

    /// Returns the value string to display.
    ///
    /// Password fields are masked as `*****`; all other types show the raw value.
    pub fn display_value(&self, value: &str) -> String {
        match self {
            FieldType::Password => "****".to_string(),
            _ => value.to_string(),
        }
    }
}

/// A single extra field (login, password, or custom) on a node.
#[derive(Debug, Clone)]
pub struct ExtraField {
    pub field_type: FieldType,
    pub value: String,
}

/// A node in the tree with a unique identifier, display name, description,
/// optional extra fields, and optional children.
#[derive(Debug, Clone)]
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

    /// Removes and returns the child with the given [`Uuid`], if present.
    #[allow(dead_code)]
    pub fn remove_child(&mut self, id: Uuid) -> Option<Node> {
        let pos = self.children.iter().position(|c| c.id == id)?;
        Some(self.children.remove(pos))
    }

    /// Inserts a child at the given position in the children list.
    #[allow(dead_code)]
    pub fn insert_child(&mut self, index: usize, child: Node) {
        self.children.insert(index, child);
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
