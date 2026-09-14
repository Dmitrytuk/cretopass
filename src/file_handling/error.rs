use std::fmt;

#[derive(Debug)]
pub enum FileHandlingError {
    Io(std::io::Error),
    Serde(serde_json::Error),
    Crypt(String),
    FileTooLarge { size: u64, max: u64 },
    FileTooSmall,
    NotAnAgeFile,
    InvalidPassphraseEncoding,
    PathNotWritable(String),
    ExportError(String),
    ImportError(String),
}

impl fmt::Display for FileHandlingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Serde(e) => write!(f, "json: {e}"),
            Self::Crypt(e) => write!(f, "crypt: {e}"),
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
            Self::ExportError(msg) => write!(f, "export: {msg}"),
            Self::ImportError(msg) => write!(f, "import: {msg}"),
        }
    }
}

impl std::error::Error for FileHandlingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serde(e) => Some(e),
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
