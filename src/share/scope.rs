/// Which part of the open Tree a share bundle contains.
///
/// The Tree offers this as a choice; the Home screen always shares the whole
/// open file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareScope {
    /// The selected Node and its descendants.
    Branch,
    /// The whole open file, taken from its Root.
    WholeFile,
}
