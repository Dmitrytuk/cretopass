//! UTF-8-aware text-editing primitives and the masked password fields built on
//! them — see [ADR 0003](docs/adr/0003-cursor.md).
//!
//! The facade re-exports two submodules with deliberately different reach:
//!
//! - `editing` — the byte-offset editing primitives and the TUI span helpers
//!   (`cursor_left`, `insert_char_at`, `cursor_field_spans`, …). Re-exported as
//!   `pub`: the tree and file-browser editing flows call them directly.
//! - `secret_field` — the masked-password `SecretField` / `SecretFieldGroup`
//!   and their span builders. Re-exported `pub(crate)`: the password prompts
//!   are the only consumers, so the public surface stays the low-level
//!   primitives.

mod editing;
mod secret_field;

pub use self::editing::*;
pub(crate) use self::secret_field::*;
