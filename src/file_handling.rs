#![warn(missing_docs)]
#![forbid(unsafe_code)]
//! The application's I/O gateway: every vault and document read/write goes
//! through this module.
//!
//! The submodules are an implementation detail; callers use the re-exports
//! below, so the module layout (which file owns which helper) can change
//! without touching call sites. The contract is documented in
//! `src/file_handling/docs/adr/0001-io-gateway.md` (ADR 0001).

pub(crate) mod atomic;
pub(crate) mod autosave;
pub(crate) mod cancel;
pub(crate) mod csv;
pub(crate) mod disks;
pub(crate) mod error;
pub(crate) mod export;
pub(crate) mod file_process;
pub(crate) mod format;
pub(crate) mod import;
pub(crate) mod passphrase;
pub(crate) mod share;
pub(crate) mod xml;

pub use autosave::{Autosave, SAVE_TIMEOUT, SaveError, SaveEvent};
pub use cancel::CancelToken;
pub use disks::{DiskInfo, list_disks};
pub use error::{FileHandlingError, SerdeContext};
pub use export::{ExportFormat, export_tree, output_path, suggest_export_name};
pub use file_process::{
    FileEntry, create_dir, create_secret_file, decrypt_to_node, delete_file, is_age_file,
    is_dir_empty, list_dir, list_dir_including_hidden, looks_like_vault, probe_path_writable,
    read_file, rename_file, save_tree, validate_encrypted_file, write_file,
};
pub use import::{ParseError, import_tree};
pub use passphrase::Passphrase;
pub use share::{share_tree, suggest_share_name};

// `pub(crate) use` marks a crate-internal seam: reached by other modules in the
// crate but not published API. A symbol with no caller outside this module is
// not re-exported here at all, so the gateway can change or drop it without a
// public-API commitment; submodule items stay reachable inside the module
// (e.g. `format::Format`).
// Secret-bearing directory creation (`vault/utils.rs`).
pub(crate) use file_process::create_private_dir;
// The cancellable vault write the autosave worker uses (`tree/app_tree.rs`).
pub(crate) use file_process::save_tree_guarded;
// The JSON writer the export path reaches through this facade.
pub(crate) use file_process::node_to_json;
