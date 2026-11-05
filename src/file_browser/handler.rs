//! The File Browser's key handling, split by responsibility.
//!
//! [`handle_key`] peels off the transient routing states (the error popup, the
//! `a` prefix, the confirmations, the Locations overlay and the search box) and
//! dispatches the rest to the handler for the focused area — the list, the name
//! input or the rename input. Committing a named destination lives in
//! [`destination`]; the text-editing map shared by the two inputs is
//! [`text_edit`]. Each submodule owns its own tests; the shared fixtures live in
//! `fixtures`.

#[cfg(test)]
mod fixtures;

mod destination;
mod input;
mod keys;
mod list;
mod locations;
mod rename;
mod text_edit;

pub use keys::{handle_click, handle_key, handle_paste};
