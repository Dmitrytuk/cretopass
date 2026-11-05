//! The File Browser's state, split by responsibility.
//!
//! [`FileBrowserState`] is the modal's whole state; [`FileBrowserMode`] decides
//! what a selection means; [`build_locations`] builds the Locations overlay rows
//! and [`types`] holds the small shared value types. Directory listings run
//! through the background `file_handling::DirLoader`, so this module only
//! requests and applies them. Each submodule owns its own tests.

mod browser;
mod locations;
mod mode;
mod types;

pub use browser::FileBrowserState;
pub use locations::{build_locations, preselect_location};
pub use mode::FileBrowserMode;
pub use types::{AddPrefix, FileBrowserAction, FileBrowserSession, FocusTarget, Location};
