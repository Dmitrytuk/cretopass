use zeroize::Zeroizing;

use crate::tree::node::Node;

/// Holds the state of the currently opened secret file.
///
/// Stored in the settings module so both the settings UI and the tree
/// app can access the shared file state without coupling them directly.
#[derive(Clone)]
pub struct ActiveFileState {
    /// Full filesystem path of the opened file.
    pub path: Option<String>,
    /// Passphrase bytes provided by the user, zeroized on drop.
    pub passphrase: Zeroizing<Vec<u8>>,
    /// The deserialized root [`Node`] that will be loaded into the tree.
    pub node: Option<Node>,
}

impl Default for ActiveFileState {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveFileState {
    pub fn new() -> Self {
        Self {
            path: None,
            passphrase: Zeroizing::new(Vec::new()),
            node: None,
        }
    }
}
