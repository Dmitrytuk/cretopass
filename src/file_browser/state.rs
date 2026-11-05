//! The File Browser's state, split by responsibility.
//!
//! [`FileBrowserState`] is the modal's whole state; [`FileBrowserMode`] decides
//! what a selection means; [`list_entries`] and [`build_locations`] are the
//! filesystem reads behind it, and [`types`] holds the small shared value types.
//! Each submodule owns its own tests.

mod browser;
mod locations;
mod mode;
mod types;

pub use browser::FileBrowserState;
pub use locations::{build_locations, list_entries, preselect_location};
pub use mode::FileBrowserMode;
pub use types::{AddPrefix, FileBrowserAction, FileBrowserSession, FocusTarget, Location};
