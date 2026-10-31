//! The Tree screen: its public API and the module tree behind it.
//!
//! This module is a facade: it declares submodules and re-exports what the rest
//! of the crate may use. It holds no logic — add tree state to `state.rs`, form
//! state to `form.rs`, autosave status to `save.rs`, drawing to `render.rs`, and
//! the windows over the tree to `modal.rs`. What a modal asks for is a
//! `TreeAction`, performed by `execute` (see ADR 0005).
//!
//! The crate's external contract is [`TreeState`], [`InputMode`], [`SaveState`]
//! and [`TreeWidget`] (re-exported by `lib.rs`). Every other item is
//! `pub(crate)`: the tree event loop (`app_tree`) and the modals are the only
//! consumers inside the crate.

mod busy;
mod form;
mod modal;
mod orchestrator;
mod render;
mod save;
mod state;

// The crate's external contract (also re-exported by `lib.rs`).
pub use render::TreeWidget;
pub use save::SaveState;
pub use state::{InputMode, TreeState};

// Internal API: the tree event loop and the modals.
pub(crate) use form::{FormFocus, FormPending, FormRow, FormState, GeneratorModal};
pub(crate) use modal::{
    Detail, ExportDestination, ExportWizard, Help, Modal, ShareDestination, SharePassphrase,
    ShareWizard, TreeAction, centered_rect,
};
pub(crate) use orchestrator::execute;
pub(crate) use save::SaveExit;
pub(crate) use state::AddPosition;

/// Items the tree's tests reach for; the production build uses them only inside
/// this module tree.
#[cfg(test)]
pub(crate) use modal::{
    GENERATOR_ROW_GENERATE, GENERATOR_ROW_LENGTH, GENERATOR_ROW_SYMBOLS, PickerItem, form_hint,
    picker_items, render_extra_field_line,
};
