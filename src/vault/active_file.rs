use zeroize::Zeroizing;

use crate::tree::node::Node;

/// Holds the state of the currently opened secret file.
///
/// Stored in the vault module so both the home screen and the tree
/// can access the shared vault state without coupling them directly.
#[derive(Clone)]
pub struct ActiveVaultState {
    /// Full filesystem path of the opened file.
    pub path: Option<String>,
    /// Master password bytes provided by the user, zeroized on drop.
    pub passphrase: Zeroizing<Vec<u8>>,
    /// The deserialized root [`Node`] that will be loaded into the tree.
    pub node: Option<Node>,
}

impl Default for ActiveVaultState {
    fn default() -> Self {
        Self::new()
    }
}

impl ActiveVaultState {
    pub fn new() -> Self {
        Self {
            path: None,
            passphrase: Zeroizing::new(Vec::new()),
            node: None,
        }
    }
}
