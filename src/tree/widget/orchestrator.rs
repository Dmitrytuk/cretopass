//! The one place that turns a modal's [`TreeAction`] into real work.
//!
//! Everything the Tree screen asks for lands here: disk I/O, the system
//! clipboard, screen transitions. The modals stay pure input (see
//! [`super::modal`]); this module is allowed to be shallow, because all it does
//! is perform intentions.

use std::io;

use super::TreeState;
use super::modal::TreeAction;

/// Performs `action`. The modal that produced it has already been taken out of
/// [`TreeState`], so dismissing it is the act of not putting it back.
pub fn execute(_state: &mut TreeState, action: TreeAction) -> io::Result<()> {
    match action {
        TreeAction::Close => Ok(()),
    }
}
