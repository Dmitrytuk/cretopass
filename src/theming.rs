pub mod icon;
pub mod key;
pub mod palette_astro_dark;
pub mod palette_cyber_orchid;
pub mod palette_default;
pub mod spinner;
pub mod status;
pub mod status_bar;
pub mod style;
pub mod texts;

// ── Active colour scheme ───────────────────────────────────────────────────
// Switch the whole UI theme by swapping the `pub use` lines below: comment the
// active scheme and uncomment one of the alternatives.
pub use palette_astro_dark as palette;
// pub use palette_cyber_orchid as palette;
// pub use palette_default as palette;

pub use icon::Icon;
pub use key::Action;
pub use spinner::{SPINNER_TICK, Spinner};
pub use status::Status;
pub use status_bar::render_status_bar;
pub use style::ElementStyle;
