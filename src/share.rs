//! The share flow's UI-agnostic pieces, shared by the Tree and the Home screen.
//!
//! Writing a bundle is [`crate::file_handling::share`]'s job. What lives here is
//! the masked password pair a bundle is encrypted with. This module never
//! renders, never touches the filesystem, and never reaches for `TreeState` or
//! `HomeState`; the screens own the chrome, the key dispatch, and the
//! screen-specific actions (`TreeAction` / `HomeAction`).

mod passphrase;

pub use passphrase::{Passphrase, PassphraseSubmit};
