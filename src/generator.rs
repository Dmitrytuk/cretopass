//! Password generation.
//!
//! A strict leaf: it depends only on `std` and `rand` and knows nothing about
//! the tree, the form, or the TUI. The tree layer maps a field type to a
//! generator and feeds it [`PasswordParams`]. See
//! `docs/adr/0012-password-generator.md`.

pub mod password;

pub use password::{
    PasswordLength, PasswordLengthError, PasswordParams, SYMBOLS, SymbolSet, generate_password,
};
