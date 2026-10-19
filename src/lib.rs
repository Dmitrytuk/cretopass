use std::io;

use crossterm::ExecutableCommand;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::{Terminal, backend::CrosstermBackend};

pub mod cli;
pub mod crypt;
pub mod cursor;
pub mod file_browser;
pub mod file_handling;
pub mod theming;
pub mod tree;
pub mod vault;

pub use tree::node::{ExtraField, FieldType, Node};
pub use tree::widget::{InputMode, SaveState, TreeState, TreeWidget};

/// Runs the home screen. Selecting "Open secret file" opens the tree
/// with the decrypted contents of the selected encrypted file.
pub fn run() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let res = vault::run(&mut terminal);
    cleanup_terminal(&mut terminal);
    if let Err(ref e) = res {
        eprintln!("Error: {e}");
    }
    res
}

/// Runs the TUI with a custom root node, skipping the home screen.
pub fn run_with_root(root: Node) -> io::Result<()> {
    let mut state = TreeState::new(root);
    let mut terminal = setup_terminal()?;
    let res = tree::app_tree::run_app(&mut terminal, &mut state, None);
    cleanup_terminal(&mut terminal);
    if let Err(ref e) = res {
        eprintln!("Error: {e}");
    }
    res
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

fn cleanup_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    let _ = disable_raw_mode();
    let _ = terminal.backend_mut().execute(LeaveAlternateScreen);
    let _ = terminal.show_cursor();
}

/// Builds a demo tree with sample data: a "Knowledge Base" root containing
/// Rust, Python, and Docker categories, each with three leaf topics.
pub fn build_demo_tree() -> Node {
    /// Attaches a Description extra field to a node and returns it.
    fn described(mut node: Node, description: &str) -> Node {
        node.extra_fields
            .push(crate::tree::node::ExtraField::description(description));
        node
    }

    let rust = described(
        Node::with_children(
            "Rust",
            vec![
                described(
                    Node::new("Ownership"),
                    "Borrow checker, lifetimes, and memory management",
                ),
                described(
                    Node::new("Traits"),
                    "Shared behavior via trait definitions and implementations",
                ),
                described(
                    Node::new("Async"),
                    "Async/await, futures, and tokio runtime",
                ),
            ],
        ),
        "Systems language focused on safety and performance",
    );

    let python = described(
        Node::with_children(
            "Python",
            vec![
                described(
                    Node::new("AsyncIO"),
                    "Event loop, coroutines, and async/await syntax",
                ),
                described(Node::new("Typing"), "Type hints, MyPy, and gradual typing"),
                described(
                    Node::new("Packaging"),
                    "pip, venv, poetry, and dependency management",
                ),
            ],
        ),
        "High-level general purpose programming language",
    );

    let docker = described(
        Node::with_children(
            "Docker",
            vec![
                described(
                    Node::new("Images"),
                    "Building custom images with Dockerfiles",
                ),
                described(
                    Node::new("Compose"),
                    "Multi-container applications with docker-compose.yml",
                ),
                described(Node::new("Volumes"), "Persistent data storage and sharing"),
            ],
        ),
        "Container platform for developing and running applications",
    );

    Node::with_children("Knowledge Base", vec![rust, python, docker])
}
