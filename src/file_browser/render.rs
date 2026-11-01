//! Rendering the File Browser modal, split by responsibility.
//!
//! [`render_file_browser`] draws the frame and stacks the popups; the file list
//! and its scroll maths live in [`list`], the status-bar badge and hint in
//! [`status`], the centered dialogs in [`popups`], and the dialog geometry in
//! [`chrome`]. Each submodule owns its own tests; the shared fixtures live in
//! [`fixtures`].

#[cfg(test)]
mod fixtures;

mod chrome;
mod list;
mod popups;
mod status;
mod window;

pub(crate) use status::status_parts;
pub use window::render_file_browser;
