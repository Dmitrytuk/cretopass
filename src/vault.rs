//! Home screen facade.
//!
//! [`run`] is the screen entry point; [`active_file`] and [`utils`] are its
//! data and configuration halves; `widget` is internal.

pub mod active_file;
pub mod utils;

mod widget;
pub use widget::run;
