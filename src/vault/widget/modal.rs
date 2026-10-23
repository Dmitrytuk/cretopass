//! The key-handling contract shared by the Home modals.
//!
//! A modal on this contract is pure input: [`Modal::handle_key`] reads a key and
//! reports what the user asked for as a [`HomeAction`]. It never touches the
//! filesystem, never draws, and never builds another modal — the orchestrator
//! performs the action. Everything else in a modal's behaviour (validation,
//! focus, masking) stays inside the modal with its own unit tests.
//!
//! This is the seam that lets the Home flows be tested without a terminal or a
//! real secret file.

mod about;
mod alert;
mod change_password;
mod configuration;
mod confirmation;
mod create_secret;
mod export;
mod unlock;

pub use about::About;
pub use alert::Alert;
pub use change_password::ChangePasswordPrompt;
pub use configuration::{ConfigurationModal, Setting};
pub use confirmation::Confirmation;
pub use create_secret::CreateSecretPrompt;
pub use export::ExportWizard;
pub use unlock::UnlockPrompt;

use crate::file_handling::export::ExportFormat;

/// What the active modal asks the Home screen to do.
///
/// Variants are added by the commit that migrates the flow producing them, so the
/// enum never carries a case nothing constructs.
pub enum HomeAction {
    /// Dismiss the modal and stay on Home.
    Close,
    /// Drop `path` from the recent secret files.
    DeleteRecent { path: String },
    /// Ask for the current, new and confirmation master passwords.
    ChangePasswordPrompt,
    /// Re-open the create dialog for `file_path`, retyping the passphrase.
    ReopenCreateSecret { file_path: String },
    /// Create `file_path` without a master password, then open it.
    CreateWithoutPassword { file_path: String },
    /// Write the open tree to a plain-text file next to the secret file.
    Export { format: ExportFormat },
    /// Flip `setting` and apply it, keeping the dialog open.
    ToggleSetting(Setting),
    /// Persist the configuration as it stands.
    SaveConfig,
    /// Reload the stored configuration, discarding the edits.
    DiscardConfig,
    /// Decrypt the file the unlock prompt was opened for, with the typed password.
    Unlock,
    /// Create the file the create dialog was opened for, with the typed password.
    CreateSecret,
    /// Ask whether to create `file_path` with no master password at all.
    ConfirmNoPassphrase { file_path: String },
    /// Re-encrypt the open file with the typed replacement master password.
    ChangeMasterPassword,
}
