use std::fmt;

use crate::crypt::CryptError;

/// A structural problem found while parsing an imported document.
///
/// Every arm is a fixed message. No value read from the imported file is ever
/// carried here, so an imported secret can never reach a user through an
/// error's [`Display`](fmt::Display).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSyntax {
    /// An XML declaration (`<?xml … ?>`) was never closed.
    UnterminatedXmlDeclaration,
    /// The XML document nests deeper than the import limit.
    XmlNestingTooDeep,
    /// A CSV path has more `>`-separated segments than the import limit.
    CsvPathTooDeep,
    /// A `<node>` element was expected.
    ExpectedNodeStart,
    /// A `<node …>` tag was never closed with `>`.
    ExpectedNodeTagEnd,
    /// A node carried no `name` attribute.
    MissingNodeName,
    /// An `<extra_fields>` element was expected.
    ExpectedExtraFieldsStart,
    /// An `</extra_fields>` element was expected.
    ExpectedExtraFieldsEnd,
    /// A `</node>` element was expected.
    ExpectedNodeEnd,
    /// A `<field>` element was expected.
    ExpectedFieldStart,
    /// A `<field …>` tag was never closed with `>`.
    ExpectedFieldTagEnd,
    /// A field carried no `type` attribute.
    MissingFieldType,
    /// A field carried no `value` attribute.
    MissingFieldValue,
    /// An attribute was missing its `=` or its closing quote.
    MalformedAttribute,
    /// The CSV document was empty.
    EmptyCsv,
    /// The CSV document had no header row.
    MissingCsvHeader,
    /// The CSV document had a header but no data rows.
    NoCsvDataRows,
    /// The CSV document produced no tree.
    NoTree,
    /// The JSON document was not a valid node.
    InvalidJson,
}

impl ImportSyntax {
    /// The human-readable detail shown after the `import:` prefix.
    pub fn message(self) -> &'static str {
        match self {
            Self::UnterminatedXmlDeclaration => "unterminated XML declaration",
            Self::XmlNestingTooDeep => "XML nesting too deep",
            Self::CsvPathTooDeep => "CSV path is too deeply nested",
            Self::ExpectedNodeStart => "expected <node",
            Self::ExpectedNodeTagEnd => "expected > in <node",
            Self::MissingNodeName => "missing name attribute",
            Self::ExpectedExtraFieldsStart => "expected <extra_fields>",
            Self::ExpectedExtraFieldsEnd => "expected </extra_fields>",
            Self::ExpectedNodeEnd => "expected </node>",
            Self::ExpectedFieldStart => "expected <field",
            Self::ExpectedFieldTagEnd => "expected > in <field>",
            Self::MissingFieldType => "missing type in <field>",
            Self::MissingFieldValue => "missing value in <field>",
            Self::MalformedAttribute => "malformed attribute",
            Self::EmptyCsv => "empty CSV file",
            Self::MissingCsvHeader => "CSV file is missing its header row",
            Self::NoCsvDataRows => "CSV file has no data rows",
            Self::NoTree => "no data to import",
            Self::InvalidJson => "not a valid JSON node",
        }
    }
}

impl fmt::Display for ImportSyntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message())
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
    /// A JSON (de)serialisation failure.
    ///
    /// The raw `serde_json` detail can quote the document that failed to parse,
    /// so it is reachable only through [`source()`](std::error::Error::source);
    /// `Display` reports a fixed message.
    Serde(serde_json::Error),
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
    PathNotWritable(String),
    /// The import path's extension maps to no supported format.
    UnsupportedImportFormat(String),
    /// An imported document was malformed.
    ImportSyntax(ImportSyntax),
    /// A CSV data row (1-based `line`, counting the header) had fewer fields
    /// than the header requires.
    CsvRowTooFewFields {
        /// The 1-based physical line the row starts on.
        line: usize,
    },
    /// An export would overwrite an age-encrypted secret file.
    ExportOverwritesSecret {
        /// The secret file that would have been overwritten.
        path: String,
    },
    /// Serialising the tree for an export failed.
    ///
    /// As with [`FileHandlingError::Serde`], the raw detail stays in
    /// [`source()`](std::error::Error::source) and out of `Display`.
    ExportSerialization(serde_json::Error),
    /// A share destination exists as something other than a folder.
    ShareDestinationNotAFolder {
        /// The destination that was not a folder.
        path: String,
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
            Self::Serde(_) => f.write_str("json: invalid document"),
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
            Self::PathNotWritable(p) => write!(f, "cannot write to path: {p}"),
            Self::UnsupportedImportFormat(p) => {
                write!(f, "import: unsupported file extension: {p}")
            }
            Self::ImportSyntax(reason) => write!(f, "import: {reason}"),
            Self::CsvRowTooFewFields { line } => {
                write!(f, "import: CSV row {line} has too few fields")
            }
            Self::ExportOverwritesSecret { path } => {
                write!(f, "export: refusing to overwrite the secret file {path}")
            }
            Self::ExportSerialization(_) => f.write_str("export: json serialisation failed"),
            Self::ShareDestinationNotAFolder { path } => {
                write!(f, "share: refusing to share into {path}: not a folder")
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
            Self::Serde(e) => Some(e),
            Self::Crypt(e) => Some(e),
            Self::ExportSerialization(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for FileHandlingError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for FileHandlingError {
    fn from(e: serde_json::Error) -> Self {
        Self::Serde(e)
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
    fn a_serde_error_keeps_the_parsed_value_out_of_display() {
        // A type mismatch makes serde_json echo the offending value, so this
        // error's detail carries a document fragment. `Display` must not.
        let serde =
            serde_json::from_str::<std::collections::BTreeMap<String, String>>("\"hunter2\"")
                .unwrap_err();
        assert!(
            serde.to_string().contains("hunter2"),
            "the fixture must embed the value: {serde}"
        );

        let error = FileHandlingError::Serde(serde);
        assert_eq!(error.to_string(), "json: invalid document");
        let cause = error.source().expect("the serde detail stays reachable");
        assert!(
            cause.to_string().contains("hunter2"),
            "source() must still carry the detail: {cause}"
        );
    }

    #[test]
    fn an_export_serialisation_error_forwards_its_cause() {
        let serde = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let error = FileHandlingError::ExportSerialization(serde);

        assert!(error.source().is_some());
    }

    #[test]
    fn an_import_syntax_error_names_no_field_value() {
        // A secret value must never appear in the message: only a fixed reason.
        let error = FileHandlingError::ImportSyntax(ImportSyntax::MissingNodeName);
        assert_eq!(error.to_string(), "import: missing name attribute");
    }

    #[test]
    fn a_short_csv_row_names_only_its_line() {
        let error = FileHandlingError::CsvRowTooFewFields { line: 42 };
        let shown = error.to_string();

        assert!(shown.contains("42"), "{shown}");
        // The message reports the line, never the row's contents.
        assert_eq!(shown, "import: CSV row 42 has too few fields");
    }

    #[test]
    fn every_variant_displays_without_panicking() {
        let serde = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let variants = [
            FileHandlingError::Io(std::io::Error::other("disk on fire")),
            FileHandlingError::Serde(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
            FileHandlingError::Crypt(CryptError::WrongPassphrase),
            FileHandlingError::FileTooLarge { size: 9, max: 1 },
            FileHandlingError::FileTooSmall,
            FileHandlingError::NotAnAgeFile,
            FileHandlingError::InvalidPassphraseEncoding,
            FileHandlingError::PathNotWritable("/root/vault.age".to_string()),
            FileHandlingError::UnsupportedImportFormat("notes.txt".to_string()),
            FileHandlingError::ImportSyntax(ImportSyntax::ExpectedNodeEnd),
            FileHandlingError::CsvRowTooFewFields { line: 3 },
            FileHandlingError::ExportOverwritesSecret {
                path: "/tmp/vault.age".to_string(),
            },
            FileHandlingError::ExportSerialization(serde),
            FileHandlingError::ShareDestinationNotAFolder {
                path: "/tmp/notes.txt".to_string(),
            },
            FileHandlingError::WriteAbandoned,
        ];

        for error in variants {
            assert!(!error.to_string().is_empty());
        }
    }
}
