use std::io;

use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::{backend::CrosstermBackend, Terminal};

pub mod crypt;
pub mod cursor;
pub mod file_browser;
pub mod file_handling;
pub mod settings;
pub mod theming;
pub mod tree;

pub use tree::node::{ExtraField, FieldType, Node};
pub use tree::widget::{InputMode, TreeState, TreeWidget};

/// Runs the settings menu. Selecting "Open secret file" opens the tree
/// app with the decrypted contents of the selected encrypted file.
pub fn run() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let res = settings::run(&mut terminal);
    cleanup_terminal(&mut terminal);
    if let Err(ref e) = res {
        eprintln!("Error: {e}");
    }
    res
}

/// Runs the TUI with a custom root node, skipping the settings menu.
pub fn run_with_root(root: Node) -> io::Result<()> {
    let mut state = TreeState::new(root);
    let mut terminal = setup_terminal()?;
    let res = tree::app_tree::run_app(&mut terminal, &mut state);
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
    Node::with_children(
        "Knowledge Base",
        "",
        vec![
            Node::with_children(
                "Rust",
                "Systems language focused on safety and performance",
                vec![
                    Node::new(
                        "Ownership",
                        "Borrow checker, lifetimes, and memory management",
                    ),
                    Node::new(
                        "Traits",
                        "Shared behavior via trait definitions and implementations",
                    ),
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
                    Node::new(
                        "Compose",
                        "Multi-container applications with docker-compose.yml",
                    ),
                    Node::new("Volumes", "Persistent data storage and sharing"),
                ],
            ),
        ],
    )
}
