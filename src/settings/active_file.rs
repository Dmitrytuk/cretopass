use crate::tree::node::Node;

/// Holds the state of the currently opened secret file.
///
/// Stored in the settings module so both the settings UI and the tree
/// app can access the shared file state without coupling them directly.
#[derive(Clone)]
pub struct ActiveFileState {
    /// Full filesystem path of the opened file.
    pub path: Option<String>,
    /// Raw passphrase bytes provided by the user.
    pub passphrase: Vec<u8>,
    /// The deserialized root [`Node`] that will be loaded into the tree.
    pub node: Option<Node>,
}

impl ActiveFileState {
    pub fn new() -> Self {
        Self {
            path: None,
            passphrase: Vec::new(),
            node: None,
        }
    }
}
