mod handler;
mod render;
mod state;

pub use handler::{handle_click, handle_key, handle_paste};
pub use render::render_file_browser;
pub(crate) use render::status_parts;
// Test-only seam: the app never names `Location`; tests inside the module tree do.
#[cfg(test)]
pub use state::Location;
pub use state::{FileBrowserAction, FileBrowserMode, FileBrowserSession, FileBrowserState};
