use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Node {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            children: Vec::new(),
        }
    }

    pub fn with_children(
        name: impl Into<String>,
        description: impl Into<String>,
        children: Vec<Node>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: description.into(),
            children,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    pub fn add_child(&mut self, child: Node) {
        self.children.push(child);
    }

    #[allow(dead_code)]
    pub fn remove_child(&mut self, id: Uuid) -> Option<Node> {
        let pos = self.children.iter().position(|c| c.id == id)?;
        Some(self.children.remove(pos))
    }

    #[allow(dead_code)]
    pub fn insert_child(&mut self, index: usize, child: Node) {
        self.children.insert(index, child);
    }

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

    pub fn contains(&self, id: Uuid) -> bool {
        if self.id == id {
            return true;
        }
        self.children.iter().any(|c| c.contains(id))
    }
}
