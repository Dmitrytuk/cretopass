//! The File Browser's state, split by responsibility.
//!
//! [`FileBrowserState`] is the modal's whole state; [`FileBrowserMode`] decides
//! what a selection means and [`types`] holds the small shared value types.
//! Directory listings and the Locations overlay's disk enumeration both run
//! through background `file_handling` loaders, so this module only requests and
//! applies them. Each submodule owns its own tests.

mod browser;
mod locations;
mod mode;
mod types;

pub use browser::FileBrowserState;
pub use mode::FileBrowserMode;
pub(crate) use types::BrowserHit;
pub use types::{AddPrefix, FileBrowserAction, FileBrowserSession, FocusTarget, Location};
