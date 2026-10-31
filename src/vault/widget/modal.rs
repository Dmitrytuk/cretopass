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
mod browser;
mod change_password;
mod configuration;
mod confirmation;
mod create_secret;
mod export;
mod share;
mod unlock;

pub use about::About;
pub use alert::Alert;
pub use browser::BrowserModal;
pub use change_password::ChangePasswordPrompt;
pub use configuration::{ConfigurationModal, Setting};
pub use confirmation::Confirmation;
pub use create_secret::CreateSecretPrompt;
pub use export::ExportWizard;
pub use share::SharePassphrase;
pub use unlock::UnlockPrompt;

use crossterm::event::KeyEvent;

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
    /// Write the open tree to `path` as plain text, in `format`.
    Export { path: String, format: ExportFormat },
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
    /// Take `from`'s place in the recent list with `to`.
    RenameFile { from: String, to: String },
    /// Merge the tree stored in `import_path` into the open vault.
    ImportFileChosen { import_path: String },
    /// The user picked an empty `file_path` to create a secret file at.
    CreateFileChosen { file_path: String },
    /// The user picked `file_path` to unlock as a secret file.
    OpenFileChosen { file_path: String },
    /// End the app.
    Quit,
    /// Leave Home: the Tree when a file is open, otherwise the app ends.
    Back,
    /// Reopen the already-decrypted vault in the Tree.
    LaunchTree,
    /// Open the file browser to pick a secret file to unlock.
    ChooseFileToOpen,
    /// Open the file browser to name a new secret file.
    ChooseFileToCreate,
    /// Open the file browser to pick something to import.
    ChooseFileToImport,
    /// Open the app identity card.
    OpenAbout,
    /// Open the persisted preferences.
    OpenConfiguration,
    /// Confirm, then ask for a new master password for the open file.
    BeginChangePassword,
    /// Choose a format, then name the destination of the export.
    BeginExport,
    /// Open the file browser to name the destination of an export in `format`.
    BeginExportDestination { format: ExportFormat },
    /// Open the file browser to name the folder a share bundle is written into.
    BeginShare,
    /// Open the share passphrase prompt for the named bundle folder.
    ShareDestinationChosen { dest: String },
    /// A passwordless share was asked for; confirm it before writing.
    ShareNoPassphrase { dest: String },
    /// Re-open the share passphrase prompt for `dest` after it was declined.
    ReopenSharePassphrase { dest: String },
    /// Write the open vault as a share bundle into `dest`, encrypted with
    /// `passphrase`.
    Share { dest: String, passphrase: String },
    /// Unlock and open `path` from the recent list.
    OpenRecent { path: String },
    /// Confirm dropping `path` from the recent list.
    RequestDeleteRecent { path: String },
}

/// The Home screen's modal slot: at most one at a time, over the menu.
pub enum Modal {
    /// The directory browser, which picks a file to unlock, name, or import.
    /// Boxed because its state is far larger than the other modals'.
    FileBrowser(Box<BrowserModal>),
    /// Unlock prompt: one masked field for the master password.
    Unlock(UnlockPrompt),
    /// Create a new secret file: the master password and its confirmation.
    CreateSecret(CreateSecretPrompt),
    /// The persisted Home preferences.
    Configuration(ConfigurationModal),
    /// A passive message the user acknowledges with any key.
    Alert(Alert),
    /// A yes/no confirmation; the prompt and its consequences follow from the
    /// kind it carries.
    Confirmation(Confirmation),
    /// Change master password: current, new, and confirmation.
    ChangePassword(ChangePasswordPrompt),
    /// The export flow's first step: pick a format.
    Export(ExportWizard),
    /// The share flow's password step: the recipient's password and its
    /// confirmation.
    SharePassphrase(SharePassphrase),
    /// App identity, version, and feedback links, opened with `?` from the menu.
    About(About),
}

impl Modal {
    /// A passive alert reporting a successful outcome.
    pub fn success(message: impl Into<String>) -> Self {
        Self::Alert(Alert::success(message))
    }

    /// A passive alert reporting a refusal or a failure.
    pub fn error(message: impl Into<String>) -> Self {
        Self::Alert(Alert::error(message))
    }

    /// Reads a key: `None` means the modal consumed it and stays open, `Some` is
    /// what the user asked for.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match self {
            Self::Alert(alert) => alert.handle_key(key),
            Self::About(about) => about.handle_key(key),
            Self::Unlock(prompt) => prompt.handle_key(key),
            Self::CreateSecret(prompt) => prompt.handle_key(key),
            Self::ChangePassword(prompt) => prompt.handle_key(key),
            Self::Configuration(modal) => modal.handle_key(key),
            Self::Confirmation(confirmation) => confirmation.handle_key(key),
            Self::Export(wizard) => wizard.handle_key(key),
            Self::SharePassphrase(prompt) => prompt.handle_key(key),
            Self::FileBrowser(browser) => browser.handle_key(key),
        }
    }
}
