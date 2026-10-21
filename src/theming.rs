pub mod background;
pub mod icon;
pub mod key;
pub mod palette_default_dark;
pub mod palette_default_light;
pub mod spinner;
pub mod status;
pub mod status_bar;
pub mod style;
pub mod texts;

// ── Active colour scheme ───────────────────────────────────────────────────
// The single colour scheme, exposed as `palette` for the rest of the UI.
pub use palette_default_dark as palette;

pub use background::paint_background;
pub use icon::Icon;
pub use key::Action;
pub use spinner::{SPINNER_TICK, Spinner};
pub use status::Status;
pub use status_bar::render_status_bar;
pub use style::ElementStyle;
