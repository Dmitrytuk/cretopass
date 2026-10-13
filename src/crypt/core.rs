use std::io::{Read, Write};

/// Encrypts a JSON string with a passphrase, returning the encrypted bytes.
///
/// The output uses the age-encryption.org/v1 binary format, which is
/// portable across all platforms and compatible with any age-compatible
/// tool (rage CLI, age CLI, etc.).
pub fn encrypt(json: &str, passphrase: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let passphrase = age::secrecy::SecretString::from(passphrase.to_owned());
    let encryptor = age::Encryptor::with_user_passphrase(passphrase);
    let mut encrypted = vec![];
    let mut writer = encryptor.wrap_output(&mut encrypted)?;
    writer.write_all(json.as_bytes())?;
    writer.finish()?;
    Ok(encrypted)
}

/// Decrypts age-encrypted bytes with a passphrase, returning the original
/// JSON string.
pub fn decrypt(encrypted: &[u8], passphrase: &str) -> Result<String, Box<dyn std::error::Error>> {
    let passphrase = age::secrecy::SecretString::from(passphrase.to_owned());
    let decryptor = age::Decryptor::new(encrypted)?;
    let identity = age::scrypt::Identity::new(passphrase);
    let mut decrypted = String::new();
    let mut reader = decryptor.decrypt(std::iter::once(&identity as &dyn age::Identity))?;
    reader.read_to_string(&mut decrypted)?;
    Ok(decrypted)
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
}
