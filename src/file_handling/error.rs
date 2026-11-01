//! The I/O gateway's error taxonomy.
//!
//! [`FileHandlingError`] is the composed error a caller of the gateway sees. It
//! is built from the layer-local errors — [`atomic::WriteError`],
//! [`import::ParseError`] and [`crypt::CryptError`] — plus the file-level
//! outcomes the gateway itself owns (size caps, age-header checks, export/share
//! refusals). No arm carries a value read from a document, so `Display` is safe
//! to show and to quote in a bug report; a wrapped cause's raw detail (which may
//! echo the document) is reachable only through
//! [`source()`](std::error::Error::source).

use std::fmt;
use std::path::PathBuf;

use super::atomic::WriteError;
use super::import::ParseError;
use crate::crypt::CryptError;

/// Which operation a JSON (de)serialisation failure belongs to.
///
/// The context lets a caller tell a vault that decrypts but does not
/// deserialise (a damaged secret) from a tree that cannot be encoded on the way
/// out (a save or export failure).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerdeContext {
    /// Decoding a decrypted vault payload into a `Node`.
    Load,
    /// Encoding a `Node` before it is encrypted and saved.
    Save,
    /// Encoding a `Node` for a plaintext export.
    Export,
}

impl SerdeContext {
    /// The fixed message `Display` shows for this context.
    fn message(self) -> &'static str {
        match self {
            Self::Load => "json: invalid document",
            Self::Save => "save: json serialisation failed",
            Self::Export => "export: json serialisation failed",
        }
    }
}

/// Every failure the I/O gateway can report.
///
/// Variants carry only non-sensitive context: a path chosen by the user, a
/// line number, a fixed syntax reason, or the wrapped cause. No imported value
/// is ever *rendered*: `Display` is safe to show and to quote in a bug report,
/// and a wrapped cause's raw detail (which may echo the document) stays
/// reachable only through [`source()`](std::error::Error::source).
#[non_exhaustive]
#[derive(Debug)]
pub enum FileHandlingError {
    /// An underlying I/O failure.
    Io(std::io::Error),
    /// A JSON (de)serialisation failure, tagged with the operation that failed.
    ///
    /// The raw `serde_json` detail can quote the document that failed to parse,
    /// so it is reachable only through [`source()`](std::error::Error::source);
    /// `Display` reports a fixed message.
    Serde {
        /// The operation whose (de)serialisation failed.
        context: SerdeContext,
        /// The wrapped `serde_json` cause.
        source: serde_json::Error,
    },
    /// An encryption or decryption failure.
    Crypt(CryptError),
    /// A file exceeded the shared size cap.
    FileTooLarge {
        /// The size seen, in bytes.
        size: u64,
        /// The cap that was exceeded, in bytes.
        max: u64,
    },
    /// A file was shorter than the age magic header.
    FileTooSmall,
    /// A file did not begin with the age magic header.
    NotAnAgeFile,
    /// A passphrase could not be represented as UTF-8.
    InvalidPassphraseEncoding,
    /// A path could not be written to.
    PathNotWritable(PathBuf),
    /// The import path carries no extension the gateway supports (or none at
    /// all). The payload is that path, so the message names the file the user
    /// chose.
    UnsupportedImportFormat(PathBuf),
    /// An imported document was malformed.
    Import(ParseError),
    /// The target of a no-clobber create already exists.
    AlreadyExists {
        /// The path that was already occupied.
        path: PathBuf,
    },
    /// An export would overwrite an age-encrypted secret file.
    ExportOverwritesSecret {
        /// The secret file that would have been overwritten.
        path: PathBuf,
    },
    /// A share destination exists as something other than a folder.
    ShareDestinationNotAFolder {
        /// The destination that was not a folder.
        path: PathBuf,
    },
    /// A write was abandoned by its owner before the commit, leaving the target
    /// untouched.
    WriteAbandoned,
}

impl fmt::Display for FileHandlingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            // The serde detail can quote the document that failed to parse, so
            // it is deliberately kept out of `Display` and reachable only
            // through `source()` (see the enum doc).
            Self::Serde { context, .. } => f.write_str(context.message()),
            Self::Crypt(e) => write!(f, "{e}"),
            Self::FileTooLarge { size, max } => {
                write!(f, "file too large: {size} bytes (max {max} bytes)")
            }
            Self::FileTooSmall => write!(f, "file too small to be an age-encrypted file"),
            Self::NotAnAgeFile => {
                write!(
                    f,
                    "not an age-encrypted file (missing age-encrypt magic header)"
                )
            }
            Self::InvalidPassphraseEncoding => write!(f, "invalid passphrase encoding"),
            Self::PathNotWritable(p) => write!(f, "cannot write to path: {}", p.display()),
            Self::UnsupportedImportFormat(p) => {
                write!(f, "import: unsupported file format: {}", p.display())
            }
            Self::Import(reason) => write!(f, "import: {reason}"),
            Self::AlreadyExists { path } => {
                write!(f, "file already exists: {}", path.display())
            }
            Self::ExportOverwritesSecret { path } => {
                write!(
                    f,
                    "export: refusing to overwrite the secret file {}",
                    path.display()
                )
            }
            Self::ShareDestinationNotAFolder { path } => {
                write!(
                    f,
                    "share: refusing to share into {}: not a folder",
                    path.display()
                )
            }
            Self::WriteAbandoned => {
                write!(f, "write abandoned before commit")
            }
        }
    }
}

impl std::error::Error for FileHandlingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serde { source, .. } => Some(source),
            Self::Crypt(e) => Some(e),
            Self::Import(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FileHandlingError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<WriteError> for FileHandlingError {
    fn from(e: WriteError) -> Self {
        match e {
            WriteError::Io(error) => Self::Io(error),
            WriteError::AlreadyExists(path) => Self::AlreadyExists { path },
            WriteError::NotWritable(path) => Self::PathNotWritable(path),
            WriteError::Abandoned => Self::WriteAbandoned,
        }
    }
}

impl From<ParseError> for FileHandlingError {
    fn from(e: ParseError) -> Self {
        Self::Import(e)
    }
}

impl From<CryptError> for FileHandlingError {
    fn from(e: CryptError) -> Self {
        Self::Crypt(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    fn serde_error() -> serde_json::Error {
        serde_json::from_str::<serde_json::Value>("{").unwrap_err()
    }

    #[test]
    fn a_crypt_error_forwards_the_full_cause_chain() {
        let error = FileHandlingError::Crypt(CryptError::CorruptFile {
            message: "the ciphertext body is damaged".to_string(),
            cause: Some(Box::new(std::io::Error::other("unexpected end of file"))),
        });

        let crypt = error.source().expect("the crypt error is forwarded");
        let cause = crypt.source().expect("the age/io cause is forwarded");
        assert_eq!(cause.to_string(), "unexpected end of file");
    }

    #[test]
    fn an_unsupported_import_format_names_the_path() {
        // The payload is the user's path, not an extension, so the message must
        // say "file format" and show the path.
        let error = FileHandlingError::UnsupportedImportFormat(PathBuf::from("/tmp/notes.txt"));
        assert_eq!(
            error.to_string(),
            "import: unsupported file format: /tmp/notes.txt"
        );
    }

    #[test]
    fn a_load_serde_error_keeps_the_parsed_value_out_of_display() {
        // A type mismatch makes serde_json echo the offending value, so this
        // error's detail carries a document fragment. `Display` must not.
        let serde =
            serde_json::from_str::<std::collections::BTreeMap<String, String>>("\"hunter2\"")
                .unwrap_err();
        assert!(
            serde.to_string().contains("hunter2"),
            "the fixture must embed the value: {serde}"
        );

        let error = FileHandlingError::Serde {
            context: SerdeContext::Load,
            source: serde,
        };
        assert_eq!(error.to_string(), "json: invalid document");
        let cause = error.source().expect("the serde detail stays reachable");
        assert!(
            cause.to_string().contains("hunter2"),
            "source() must still carry the detail: {cause}"
        );
    }

    #[test]
    fn a_serde_context_names_the_failed_operation() {
        assert_eq!(
            FileHandlingError::Serde {
                context: SerdeContext::Save,
                source: serde_error(),
            }
            .to_string(),
            "save: json serialisation failed"
        );
        assert_eq!(
            FileHandlingError::Serde {
                context: SerdeContext::Export,
                source: serde_error(),
            }
            .to_string(),
            "export: json serialisation failed"
        );
    }

    #[test]
    fn a_serde_error_forwards_its_cause() {
        let error = FileHandlingError::Serde {
            context: SerdeContext::Export,
            source: serde_error(),
        };

        assert!(error.source().is_some());
    }

    #[test]
    fn an_import_syntax_error_names_no_field_value() {
        // A secret value must never appear in the message: only a fixed reason.
        let error = FileHandlingError::Import(ParseError::MissingNodeName);
        assert_eq!(error.to_string(), "import: missing name attribute");
    }

    #[test]
    fn a_short_csv_row_names_only_its_line() {
        let error = FileHandlingError::Import(ParseError::CsvRowTooFewFields { line: 42 });
        let shown = error.to_string();

        assert!(shown.contains("42"), "{shown}");
        // The message reports the line, never the row's contents.
        assert_eq!(shown, "import: CSV row 42 has too few fields");
    }

    #[test]
    fn an_already_existing_target_names_the_path() {
        let error = FileHandlingError::AlreadyExists {
            path: PathBuf::from("/tmp/vault.age"),
        };

        assert_eq!(error.to_string(), "file already exists: /tmp/vault.age");
    }

    #[test]
    fn a_write_error_composes_into_the_facade_error() {
        let cases = [
            (
                WriteError::Io(std::io::Error::other("disk on fire")),
                "io: disk on fire",
            ),
            (
                WriteError::AlreadyExists(PathBuf::from("/tmp/vault.age")),
                "file already exists: /tmp/vault.age",
            ),
            (
                WriteError::NotWritable(PathBuf::from("/tmp/vault.age")),
                "cannot write to path: /tmp/vault.age",
            ),
            (WriteError::Abandoned, "write abandoned before commit"),
        ];

        for (write_error, expected) in cases {
            assert_eq!(FileHandlingError::from(write_error).to_string(), expected);
        }
    }

    #[test]
    fn every_variant_displays_without_panicking() {
        let variants = [
            FileHandlingError::Io(std::io::Error::other("disk on fire")),
            FileHandlingError::Serde {
                context: SerdeContext::Load,
                source: serde_error(),
            },
            FileHandlingError::Crypt(CryptError::WrongPassphrase),
            FileHandlingError::FileTooLarge { size: 9, max: 1 },
            FileHandlingError::FileTooSmall,
            FileHandlingError::NotAnAgeFile,
            FileHandlingError::InvalidPassphraseEncoding,
            FileHandlingError::PathNotWritable(PathBuf::from("/root/vault.age")),
            FileHandlingError::UnsupportedImportFormat(PathBuf::from("notes.txt")),
            FileHandlingError::Import(ParseError::ExpectedNodeEnd),
            FileHandlingError::AlreadyExists {
                path: PathBuf::from("/tmp/vault.age"),
            },
            FileHandlingError::ExportOverwritesSecret {
                path: PathBuf::from("/tmp/vault.age"),
            },
            FileHandlingError::ShareDestinationNotAFolder {
                path: PathBuf::from("/tmp/notes.txt"),
            },
            FileHandlingError::WriteAbandoned,
        ];

        for error in variants {
            assert!(!error.to_string().is_empty());
        }
    }
}
