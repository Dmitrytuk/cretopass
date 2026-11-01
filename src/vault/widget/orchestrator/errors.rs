//! One place that turns a file error into the message a Home modal shows.
//!
//! The wording lives in [`texts`], but the *decision* of what a failure means to
//! a user belongs here, at the orchestrator layer — `file_handling` stays
//! presentation-free and `theming` stays a dependency-free leaf.

use crate::crypt::CryptError;
use crate::file_handling::FileHandlingError;
use crate::theming::texts;

/// The text a Home modal shows for `error`.
///
/// Only the outcomes a user can act on are named: a wrong master password, a
/// secret file that will not open, or one that needs more scrypt work than
/// this device allows. Everything else keeps its raw detail behind a generic
/// prefix, so an unexpected failure is never shown blank.
pub(super) fn message(error: &FileHandlingError) -> String {
    match error {
        FileHandlingError::Crypt(CryptError::WrongPassphrase) => {
            texts::MSG_WRONG_MASTER_PW.to_string()
        }
        // A file that decrypts but does not hold a Node is as unreadable to the
        // user as one whose header is damaged.
        FileHandlingError::Crypt(CryptError::CorruptFile { .. }) | FileHandlingError::Serde(_) => {
            texts::MSG_DAMAGED_SECRET.to_string()
        }
        FileHandlingError::Crypt(CryptError::WorkFactorTooHigh { .. }) => {
            texts::MSG_WORK_FACTOR_TOO_HIGH.to_string()
        }
        FileHandlingError::NotAnAgeFile | FileHandlingError::FileTooSmall => {
            texts::MSG_NOT_SECRET_FILE.to_string()
        }
        FileHandlingError::Crypt(CryptError::EncryptionFailed { .. }) => {
            format!("{}{error}", texts::MSG_SAVE_FAILED)
        }
        FileHandlingError::PathNotWritable(_) => texts::MSG_CANNOT_WRITE.to_string(),
        // `create_secret_file` finishes with a no-clobber rename, so a lost race
        // surfaces here instead of the caller-side `Path::exists()` check.
        FileHandlingError::Io(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            texts::MSG_FILE_EXISTS.to_string()
        }
        other => format!("{}{other}", texts::MSG_FILE_ERROR),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message_for(error: FileHandlingError) -> String {
        message(&error)
    }

    #[test]
    fn a_wrong_master_password_is_named_plainly() {
        assert_eq!(
            message_for(FileHandlingError::Crypt(CryptError::WrongPassphrase)),
            texts::MSG_WRONG_MASTER_PW
        );
    }

    #[test]
    fn a_damaged_ciphertext_is_reported_as_damaged() {
        assert_eq!(
            message_for(FileHandlingError::Crypt(CryptError::CorruptFile {
                message: "header is invalid".to_string(),
                cause: None,
            })),
            texts::MSG_DAMAGED_SECRET
        );
    }

    #[test]
    fn a_file_above_the_work_factor_limit_names_the_limit() {
        assert_eq!(
            message_for(FileHandlingError::Crypt(CryptError::WorkFactorTooHigh {
                required: 40,
                limit: 20,
            })),
            texts::MSG_WORK_FACTOR_TOO_HIGH
        );
    }

    #[test]
    fn a_decrypted_payload_that_is_not_a_node_is_reported_as_damaged() {
        let error =
            FileHandlingError::Serde(serde_json::from_str::<serde_json::Value>("{").unwrap_err());
        assert_eq!(message_for(error), texts::MSG_DAMAGED_SECRET);
    }

    #[test]
    fn a_file_that_is_not_an_age_file_is_reported_as_such() {
        assert_eq!(
            message_for(FileHandlingError::NotAnAgeFile),
            texts::MSG_NOT_SECRET_FILE
        );
        assert_eq!(
            message_for(FileHandlingError::FileTooSmall),
            texts::MSG_NOT_SECRET_FILE
        );
    }

    #[test]
    fn a_failed_encryption_reads_as_a_failed_save() {
        let error = FileHandlingError::Crypt(CryptError::EncryptionFailed {
            message: "device full".to_string(),
            cause: None,
        });
        let expected = format!("{}{}", texts::MSG_SAVE_FAILED, error);
        assert_eq!(message_for(error), expected);
    }

    #[test]
    fn a_path_that_cannot_be_written_is_reported_as_such() {
        let error = FileHandlingError::PathNotWritable("/root/vault.age".to_string());
        assert_eq!(message_for(error), texts::MSG_CANNOT_WRITE);
    }

    #[test]
    fn an_already_existing_target_is_reported_as_existing() {
        let error = FileHandlingError::Io(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "File exists",
        ));
        assert_eq!(message_for(error), texts::MSG_FILE_EXISTS);
    }

    #[test]
    fn a_malformed_import_is_not_shown_as_a_damaged_secret() {
        // A bad import file is a syntax problem; only a vault that fails to
        // deserialise after decryption is "damaged".
        let error =
            FileHandlingError::ImportSyntax(crate::file_handling::ImportSyntax::InvalidJson);

        let shown = message(&error);

        assert_eq!(shown, format!("{}{}", texts::MSG_FILE_ERROR, error));
        assert_ne!(shown, texts::MSG_DAMAGED_SECRET);
    }

    #[test]
    fn an_unexpected_failure_keeps_its_detail_behind_a_prefix() {
        let error = FileHandlingError::Io(std::io::Error::other("disk on fire"));
        let shown = message_for(error);
        assert!(shown.starts_with(texts::MSG_FILE_ERROR));
        assert!(shown.contains("disk on fire"));
    }
}
