//! The Tree event loop: its public API and the module tree behind it.
//!
//! This module is a facade: it declares submodules and re-exports what the rest
//! of the crate may use. It holds no logic — the async driver lives in
//! `app_tree/loop.rs`, input routing in `app_tree/input.rs`, and drawing in
//! `app_tree/render.rs`.
//!
//! The crate's only entry point is [`run_app`], which owns the terminal for the
//! duration of a Tree session (see [`SaveTarget`] / [`ExitStatus`]). Every other
//! item is `pub(super)`/private: the submodules are the only consumers.

mod input;
mod r#loop;
mod render;
#[cfg(test)]
mod tests;

pub use r#loop::{ExitStatus, SaveTarget, run_app};
