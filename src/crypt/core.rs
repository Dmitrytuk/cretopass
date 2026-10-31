use std::fmt;
use std::io::{Read, Write};

/// Why a [`encrypt`] or [`decrypt`] call failed.
///
/// The variants exist so a caller can tell "you typed the wrong master
/// password" from "the file will not open" without parsing a message. age
/// unwraps the file key eagerly, but verifies the ciphertext body lazily while
/// the stream is read, so a wrong passphrase and a damaged body land in
/// different variants.
#[derive(Debug)]
pub enum CryptError {
    /// The passphrase did not decrypt the file's key.
    WrongPassphrase,
    /// The file is structurally damaged, or not a format age can read.
    CorruptFile(String),
    /// Encryption failed while producing the output.
    EncryptionFailed(String),
}

impl fmt::Display for CryptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPassphrase => write!(f, "wrong master password"),
            Self::CorruptFile(message) => write!(f, "{message}"),
            Self::EncryptionFailed(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CryptError {}

/// Encrypts a JSON string with a passphrase, returning the encrypted bytes.
///
/// The output uses the age-encryption.org/v1 binary format, which is
/// portable across all platforms and compatible with any age-compatible
/// tool (rage CLI, age CLI, etc.).
pub fn encrypt(json: &str, passphrase: &str) -> Result<Vec<u8>, CryptError> {
    let passphrase = age::secrecy::SecretString::from(passphrase.to_owned());
    let encryptor = age::Encryptor::with_user_passphrase(passphrase);
    let mut encrypted = vec![];
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(failed_encryption)?;
    writer
        .write_all(json.as_bytes())
        .map_err(failed_encryption)?;
    writer.finish().map_err(failed_encryption)?;
    Ok(encrypted)
}

/// Wraps an I/O failure from the age writer as [`CryptError::EncryptionFailed`].
fn failed_encryption(error: std::io::Error) -> CryptError {
    CryptError::EncryptionFailed(error.to_string())
}

/// Decrypts age-encrypted bytes with a passphrase, returning the original
/// JSON string.
pub fn decrypt(encrypted: &[u8], passphrase: &str) -> Result<String, CryptError> {
    let passphrase = age::secrecy::SecretString::from(passphrase.to_owned());
    let decryptor = age::Decryptor::new(encrypted).map_err(classify)?;
    let identity = age::scrypt::Identity::new(passphrase);
    let mut decrypted = String::new();
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(classify)?;
    reader.read_to_string(&mut decrypted).map_err(|e| {
        // The file key unwrapped — so the passphrase was right — and age
        // verifies the ciphertext body lazily while the stream is read. A
        // failure here means the body is damaged (truncated or tampered).
        CryptError::CorruptFile(format!("damaged ciphertext body: {e}"))
    })?;
    Ok(decrypted)
}

/// Sorts an eager age failure into "the master password is wrong" or "the file
/// is damaged".
///
/// This runs before the body is read, so it only ever sees header problems and
/// the file-key unwrap; body damage is reported by [`decrypt`] itself.
fn classify(error: age::DecryptError) -> CryptError {
    use age::DecryptError;

    match error {
        DecryptError::DecryptionFailed
        | DecryptError::KeyDecryptionFailed
        | DecryptError::NoMatchingKeys => CryptError::WrongPassphrase,
        other => CryptError::CorruptFile(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt() {
        let json = r#"{"name": "Test", "value": 42}"#;
        let passphrase = "my-strong-passphrase-123";

        let encrypted = encrypt(json, passphrase).unwrap();
        let decrypted = decrypt(&encrypted, passphrase).unwrap();

        assert_eq!(decrypted, json);
    }

    #[test]
    fn test_different_passphrase() {
        let json = "secret data";
        let encrypted = encrypt(json, "correct-passphrase").unwrap();
        let result = decrypt(&encrypted, "wrong-passphrase");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_string() {
        let json = "";
        let passphrase = "test-pass";

        let encrypted = encrypt(json, passphrase).unwrap();
        let decrypted = decrypt(&encrypted, passphrase).unwrap();

        assert_eq!(decrypted, json);
    }

    #[test]
    fn test_tampered_data() {
        let json = "sensitive content";
        let passphrase = "tamper-test";

        let mut encrypted = encrypt(json, passphrase).unwrap();
        // Flip a byte in the encrypted output
        if let Some(b) = encrypted.last_mut() {
            *b ^= 0xff;
        }
        let result = decrypt(&encrypted, passphrase);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_header() {
        let json = "test";
        let encrypted = encrypt(json, "header-check").unwrap();
        // age binary format starts with "age-encrypt" magic bytes
        assert!(encrypted.starts_with(b"age-encrypt"));
    }

    #[test]
    fn test_large_data() {
        let json = "a".repeat(10_000);
        let passphrase = "large-data-test";

        let encrypted = encrypt(&json, passphrase).unwrap();
        let decrypted = decrypt(&encrypted, passphrase).unwrap();

        assert_eq!(decrypted.len(), 10_000);
        assert_eq!(decrypted, json);
    }

    #[test]
    fn test_unicode_passphrase() {
        let json = "unicode content ✓";
        let passphrase = "пароль🔑";

        let encrypted = encrypt(json, passphrase).unwrap();
        let decrypted = decrypt(&encrypted, passphrase).unwrap();

        assert_eq!(decrypted, json);
    }

    #[test]
    fn a_wrong_passphrase_is_reported_as_wrong_passphrase() {
        let encrypted = encrypt("secret", "right-pass").unwrap();
        assert!(matches!(
            decrypt(&encrypted, "wrong-pass"),
            Err(CryptError::WrongPassphrase)
        ));
    }

    #[test]
    fn a_tampered_body_is_reported_as_a_corrupt_file() {
        // The file key unwraps (the passphrase is right) and the body fails
        // later, so body damage is distinguishable from a wrong passphrase.
        let mut encrypted = encrypt("secret", "right-pass").unwrap();
        let last = encrypted.last_mut().unwrap();
        *last ^= 0xff;
        assert!(matches!(
            decrypt(&encrypted, "right-pass"),
            Err(CryptError::CorruptFile(_))
        ));
    }

    #[test]
    fn a_truncated_body_is_reported_as_a_corrupt_file() {
        let encrypted = encrypt("secret", "right-pass").unwrap();
        let truncated = &encrypted[..encrypted.len() - 4];
        assert!(matches!(
            decrypt(truncated, "right-pass"),
            Err(CryptError::CorruptFile(_))
        ));
    }

    #[test]
    fn a_damaged_header_is_reported_as_a_corrupt_file() {
        let mut encrypted = encrypt("secret", "right-pass").unwrap();
        // Corrupt the stanza that carries the encrypted file key.
        encrypted[40] ^= 0xff;
        assert!(matches!(
            decrypt(&encrypted, "right-pass"),
            Err(CryptError::CorruptFile(_))
        ));
    }
}
