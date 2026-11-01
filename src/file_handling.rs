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
pub(crate) mod disks;
pub(crate) mod error;
pub(crate) mod export;
pub(crate) mod file_process;
pub(crate) mod format;
pub(crate) mod import;
pub(crate) mod share;

pub use autosave::{Autosave, SAVE_TIMEOUT, SaveError, SaveEvent, SaveResult};
pub use disks::{DiskInfo, list_disks};
pub use error::{FileHandlingError, ImportSyntax};
pub use export::{ExportFormat, export_tree, output_path, suggest_export_name};
pub use file_process::{
    FileEntry, create_dir, create_secret_file, decrypt_to_node, delete_file, is_age_file,
    is_dir_empty, json_to_node, list_dir, list_dir_including_hidden, node_to_json,
    probe_path_writable, read_file, rename_file, save_tree, validate_encrypted_file, write_file,
};
// Crate-internal only: secret-bearing directory creation, not part of the
// published surface.
pub(crate) use file_process::create_private_dir;
pub use format::Format;
pub use import::{ImportFormat, import_tree};
pub use share::{SHARE_FILE_EXT, SHARE_README_NAME, share_tree, suggest_share_name};
