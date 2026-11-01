use std::fmt;

use crate::crypt::CryptError;

#[derive(Debug)]
pub enum FileHandlingError {
    Io(std::io::Error),
    Serde(serde_json::Error),
    Crypt(CryptError),
    FileTooLarge { size: u64, max: u64 },
    FileTooSmall,
    NotAnAgeFile,
    InvalidPassphraseEncoding,
    PathNotWritable(String),
    ExportError(String),
    ImportError(String),
    ShareError(String),
}

impl fmt::Display for FileHandlingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::Serde(e) => write!(f, "json: {e}"),
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
            Self::ExportError(msg) => write!(f, "export: {msg}"),
            Self::ImportError(msg) => write!(f, "import: {msg}"),
            Self::ShareError(msg) => write!(f, "share: {msg}"),
        }
    }
}

impl std::error::Error for FileHandlingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Serde(e) => Some(e),
            Self::Crypt(e) => Some(e),
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
}
