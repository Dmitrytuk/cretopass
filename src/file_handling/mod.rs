use std::io;
use std::fs;

use crate::tree::node::Node;

/// Serializes a [`Node`] tree to a pretty-printed JSON string.
pub fn node_to_json(node: &Node) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(node)
}

/// Deserializes a [`Node`] tree from a JSON string.
pub fn json_to_node(json: &str) -> Result<Node, serde_json::Error> {
    serde_json::from_str(json)
}

/// Serializes a [`Node`] tree and writes it to a file at the given path.
pub fn write_node_to_file(node: &Node, path: &str) -> io::Result<()> {
    let json = node_to_json(node).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, e)
    })?;
    fs::write(path, json)
}

/// Reads a file at the given path and deserializes its contents into a
/// [`Node`] tree.
pub fn read_node_from_file(path: &str) -> io::Result<Node> {
    let json = fs::read_to_string(path)?;
    json_to_node(&json).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, e)
    })
}

/// Creates a new secret file at the given path with a minimal root node.
pub fn create_secret_file(path: &str) -> io::Result<()> {
    let node = Node::new("Root", "New password database");
    write_node_to_file(&node, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::node::{ExtraField, FieldType};

    #[test]
    fn test_json_roundtrip() {
        let node = Node::with_children(
            "Root",
            "description",
            vec![
                Node::new("Child1", "first child"),
                Node::new("Child2", "second child"),
            ],
        );
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.id, decoded.id);
        assert_eq!(node.name, decoded.name);
        assert_eq!(node.description, decoded.description);
        assert_eq!(node.children.len(), decoded.children.len());
    }

    #[test]
    fn test_json_roundtrip_empty() {
        let node = Node::new("", "");
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.id, decoded.id);
        assert_eq!(node.name, decoded.name);
        assert_eq!(node.description, decoded.description);
        assert!(decoded.children.is_empty());
        assert!(decoded.extra_fields.is_empty());
    }

    #[test]
    fn test_json_roundtrip_with_extra() {
        let mut node = Node::new("Entry", "Login credentials");
        node.extra_fields = vec![
            ExtraField { field_type: FieldType::Login, value: "admin".to_string() },
            ExtraField { field_type: FieldType::Password, value: "secret123".to_string() },
            ExtraField { field_type: FieldType::Custom("Note".to_string()), value: "important".to_string() },
        ];
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.extra_fields.len(), decoded.extra_fields.len());
        for (a, b) in node.extra_fields.iter().zip(decoded.extra_fields.iter()) {
            assert_eq!(a.value, b.value);
            match (&a.field_type, &b.field_type) {
                (FieldType::Login, FieldType::Login) => {}
                (FieldType::Password, FieldType::Password) => {}
                (FieldType::Custom(an), FieldType::Custom(bn)) => assert_eq!(an, bn),
                _ => panic!("field type mismatch"),
            }
        }
    }

    #[test]
    fn test_json_roundtrip_nested() {
        let node = Node::with_children(
            "Root",
            "",
            vec![
                Node::with_children(
                    "Category",
                    "",
                    vec![
                        Node::new("Item1", "leaf"),
                        Node::new("Item2", "leaf"),
                    ],
                ),
                Node::new("Sibling", "leaf"),
            ],
        );
        let json = node_to_json(&node).unwrap();
        let decoded: Node = json_to_node(&json).unwrap();
        assert_eq!(node.id, decoded.id);
        assert_eq!(node.children.len(), decoded.children.len());
        let cat = &decoded.children[0];
        assert_eq!(cat.children.len(), 2);
    }

    #[test]
    fn test_write_read_file() {
        let node = Node::new("TestNode", "file roundtrip");
        let path = format!("/tmp/test_file_handling_{}.kdbx", uuid::Uuid::new_v4());
        let _ = fs::remove_file(&path);

        write_node_to_file(&node, &path).unwrap();
        let decoded = read_node_from_file(&path).unwrap();

        assert_eq!(node.id, decoded.id);
        assert_eq!(node.name, decoded.name);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_create_secret_file() {
        let path = format!("/tmp/test_create_{}.kdbx", uuid::Uuid::new_v4());
        let _ = fs::remove_file(&path);

        create_secret_file(&path).unwrap();
        assert!(fs::metadata(&path).is_ok());

        let decoded = read_node_from_file(&path).unwrap();
        assert_eq!(decoded.name, "Root");
        assert_eq!(decoded.description, "New password database");

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn test_invalid_json() {
        let result = json_to_node("this is not valid json");
        assert!(result.is_err());

        let result = json_to_node(r#"{"name": "incomplete"}"#);
        assert!(result.is_err());
    }
}
