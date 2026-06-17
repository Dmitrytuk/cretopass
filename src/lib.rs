pub mod tree;

pub use tree::node::{ExtraField, FieldType, Node};
pub use tree::widget::{InputMode, TreeState, TreeWidget};

/// Builds a demo tree with sample data: a "Knowledge Base" root containing
/// Rust, Python, and Docker categories, each with three leaf topics.
pub fn build_demo_tree() -> Node {
    Node::with_children(
        "Knowledge Base",
        "",
        vec![
            Node::with_children(
                "Rust",
                "Systems language focused on safety and performance",
                vec![
                    Node::new("Ownership", "Borrow checker, lifetimes, and memory management"),
                    Node::new("Traits", "Shared behavior via trait definitions and implementations"),
                    Node::new("Async", "Async/await, futures, and tokio runtime"),
                ],
            ),
            Node::with_children(
                "Python",
                "High-level general purpose programming language",
                vec![
                    Node::new("AsyncIO", "Event loop, coroutines, and async/await syntax"),
                    Node::new("Typing", "Type hints, MyPy, and gradual typing"),
                    Node::new("Packaging", "pip, venv, poetry, and dependency management"),
                ],
            ),
            Node::with_children(
                "Docker",
                "Container platform for developing and running applications",
                vec![
                    Node::new("Images", "Building custom images with Dockerfiles"),
                    Node::new("Compose", "Multi-container applications with docker-compose.yml"),
                    Node::new("Volumes", "Persistent data storage and sharing"),
                ],
            ),
        ],
    )
}
