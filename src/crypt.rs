//! Passphrase-based encryption and decryption built on the [`age`] format.
//!
//! The module wraps `age`'s scrypt recipient/identity in two functions:
//! [`encrypt`] turns UTF-8 text into age bytes, and [`decrypt`] turns them back.
//! It is a pure leaf: it depends on no other project module, and it never
//! touches the filesystem, so a real I/O error cannot be mistaken for a
//! damaged ciphertext.
//!
//! [`CryptError`] names the outcomes a caller can act on. age unwraps the file
//! key eagerly but verifies the ciphertext body lazily, so a wrong passphrase
//! and a damaged body surface as different variants; a file that asks for more
//! scrypt work than this build accepts has its own variant, so a hostile file
//! is never reported as merely "damaged".

mod core;

pub use self::core::{CryptError, decrypt, encrypt};
