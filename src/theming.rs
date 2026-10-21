pub mod icon;
pub mod key;
pub mod palette_cyber_orchid;
pub mod palette_default;
pub mod spinner;
pub mod status;
pub mod status_bar;
pub mod style;
pub mod texts;

// ── Active colour scheme ───────────────────────────────────────────────────
// Switch the whole UI theme by changing the line below to
// `pub use palette_cyber_orchid as palette;`.
pub use palette_cyber_orchid as palette;
// pub use palette_default as palette;

pub use icon::Icon;
pub use key::Action;
pub use spinner::{SPINNER_TICK, Spinner};
pub use status::Status;
pub use status_bar::render_status_bar;
pub use style::ElementStyle;
