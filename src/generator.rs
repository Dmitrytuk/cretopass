//! Value generation.
//!
//! A strict leaf: it depends only on `std` and `rand` and knows nothing about
//! the tree, the form, or the TUI. The tree layer maps a field type to a
//! generator and feeds it the parameters it needs — [`PasswordParams`] for
//! passwords, nothing for login words. See
//! `docs/adr/0012-password-generator.md` and
//! `docs/adr/0013-login-word-generator.md`.
//!
//! # Examples
//!
//! ```text
//! let password = generate_password(&PasswordParams::default());
//! let login = generate_login();
//! ```
//!
//! The crate has no library target, so these examples are prose sketches and
//! are never compiled as doctests.

mod login;
mod password;

pub use login::generate_login;
// Test-only seam: the app never names `SymbolSet`; tests inside the module tree do.
#[cfg(test)]
pub use password::SymbolSet;
pub use password::{
    PasswordLength, PasswordLengthError, PasswordParams, SYMBOLS, generate_password,
};
