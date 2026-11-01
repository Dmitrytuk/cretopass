//! The serialised-document formats the gateway imports and exports.
//!
//! Import detects a format from a file extension; export turns a format into
//! one. Both directions live on [`Format`] so the extension table has a single
//! source of truth.

use std::path::Path;

/// A serialised-document format accepted by import and export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The cretopass-native JSON document.
    Json,
    /// The XML interchange document.
    Xml,
    /// The flattened CSV interchange document.
    Csv,
}

impl Format {
    /// The extension a document of this format is written with.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Csv => "csv",
        }
    }

    /// The format a path's extension selects, or `None` when it is not one of
    /// the supported extensions.
    pub fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path).extension()?.to_str()?.to_lowercase();
        match ext.as_str() {
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}
