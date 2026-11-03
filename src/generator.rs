//! Value generation.
//!
//! A strict leaf: it depends only on `std` and `rand` and knows nothing about
//! the tree, the form, or the TUI. The tree layer maps a field type to a
//! generator and feeds it the parameters it needs — [`PasswordParams`] for
//! passwords, nothing for login words. See
//! `docs/adr/0012-password-generator.md` and
//! `docs/adr/0013-login-word-generator.md`.

mod login;
mod password;

pub use login::generate_login;
pub use password::{
    PasswordLength, PasswordLengthError, PasswordParams, SYMBOLS, SymbolSet, generate_password,
};
