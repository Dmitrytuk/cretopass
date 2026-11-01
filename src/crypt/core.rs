use std::error::Error;
use std::fmt;
use std::io::{Read, Write};

use zeroize::Zeroizing;

/// The largest scrypt work factor `decrypt` accepts, as `log2(N)`.
///
/// age tunes the work factor of a file to take roughly one second on the
/// machine that creates it, so a file made on a much faster machine — or a
/// crafted file — can ask for far more work than its content is worth.
/// cretopass is interactive, so it accepts at most `20`: enough for vaults and
/// share bundles produced on desktop machines, while bounding a hostile file
/// to `128 * 2^20 * 8` bytes (~1 GiB) and a few seconds of CPU.
///
/// The reference age implementation caps the factor at `22` (~15 s on a modern
/// machine); cretopass stays below that because it opens untrusted files handed
/// to it (share bundles, arbitrary `.age` files).
const MAX_WORK_FACTOR: u8 = 20;

/// Why an [`encrypt`] or [`decrypt`] call failed.
///
/// The variants exist so a caller can tell "you typed the wrong master
/// password" from "the file will not open" without parsing a message. age
/// unwraps the file key eagerly, but verifies the ciphertext body lazily while
/// the stream is read, so a wrong passphrase and a damaged body land in
/// different variants.
///
/// Because a scrypt salt is authenticated only by the passphrase-derived key,
/// [`Self::WrongPassphrase`] also covers a header that was altered in a way
/// that keeps its fields well-formed; the user-facing wording says so.
#[derive(Debug)]
#[non_exhaustive]
pub enum CryptError {
    /// The passphrase did not decrypt the file's key. This is usually a wrong
    /// password, but a header altered in a base64-valid way lands here too.
    WrongPassphrase,
    /// The file asks for a scrypt work factor above this build's limit.
    ///
    /// This is not a wrong passphrase: an intact file configured for a faster
    /// machine reaches here as well as a file crafted to make opening it
    /// expensive.
    WorkFactorTooHigh {
        /// The work factor `log2(N)` the file requires.
        required: u8,
        /// The largest work factor this build accepts.
        limit: u8,
    },
    /// The file is structurally damaged, or not a format age can read.
    CorruptFile {
        /// Human-readable context for the failure.
        message: String,
        /// The underlying library error, when one is available.
        cause: Option<Box<dyn Error + Send + Sync + 'static>>,
    },
    /// Encryption failed while producing the output.
    EncryptionFailed {
        /// Human-readable context for the failure.
        message: String,
        /// The underlying library error, when one is available.
        cause: Option<Box<dyn Error + Send + Sync + 'static>>,
    },
}

impl fmt::Display for CryptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPassphrase => write!(f, "wrong master password"),
            Self::WorkFactorTooHigh { required, limit } => write!(
                f,
                "the file needs scrypt work factor {required}, but this build accepts at most {limit}"
            ),
            Self::CorruptFile { message, .. } => write!(f, "corrupt file: {message}"),
            Self::EncryptionFailed { message, .. } => write!(f, "encryption failed: {message}"),
        }
    }
}

impl Error for CryptError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::WrongPassphrase | Self::WorkFactorTooHigh { .. } => None,
            Self::CorruptFile { cause, .. } | Self::EncryptionFailed { cause, .. } => cause
                .as_ref()
                .map(|error| error.as_ref() as &(dyn Error + 'static)),
        }
    }
}

/// Encrypts UTF-8 text with a passphrase, returning the encrypted bytes.
///
/// The output uses the age-encryption.org/v1 binary format, which is portable
/// across all platforms and compatible with any age-compatible tool (the
/// `rage` and `age` CLIs, etc.).
///
/// # Errors
///
/// Returns [`CryptError::EncryptionFailed`] when the underlying `age` writer
/// cannot produce the output.
pub fn encrypt(plaintext: &str, passphrase: &str) -> Result<Vec<u8>, CryptError> {
    let passphrase = age::secrecy::SecretString::from(passphrase);
    let encryptor = age::Encryptor::with_user_passphrase(passphrase);
    let mut encrypted = Vec::with_capacity(plaintext.len() + 256);
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(failed_encryption)?;
    writer
        .write_all(plaintext.as_bytes())
        .map_err(failed_encryption)?;
    writer.finish().map_err(failed_encryption)?;
    Ok(encrypted)
}

/// Wraps an I/O failure from the age writer as [`CryptError::EncryptionFailed`].
fn failed_encryption(error: std::io::Error) -> CryptError {
    CryptError::EncryptionFailed {
        message: "the output could not be written".to_string(),
        cause: Some(Box::new(error)),
    }
}

/// Decrypts age-encrypted bytes with a passphrase, returning the UTF-8 text.
///
/// # Errors
///
/// - [`CryptError::WrongPassphrase`] when the passphrase does not unwrap the
///   file key, or when a header field was altered but is still well-formed.
/// - [`CryptError::WorkFactorTooHigh`] when the file's scrypt work factor
///   exceeds this build's limit.
/// - [`CryptError::CorruptFile`] when the header or ciphertext body is damaged,
///   or when the decrypted payload is not UTF-8 text.
pub fn decrypt(encrypted: &[u8], passphrase: &str) -> Result<String, CryptError> {
    let passphrase = age::secrecy::SecretString::from(passphrase);
    let decryptor = age::Decryptor::new(encrypted).map_err(classify)?;
    let mut identity = age::scrypt::Identity::new(passphrase);
    identity.set_max_work_factor(MAX_WORK_FACTOR);
    let mut reader = decryptor
        .decrypt(std::iter::once(&identity as &dyn age::Identity))
        .map_err(classify)?;
    // The file key unwrapped — so the passphrase was right — and age verifies
    // the ciphertext body lazily while the stream is read. A failure here means
    // the body is damaged (truncated or tampered) or the payload is not UTF-8.
    // Keep the partial plaintext in a `Zeroizing` buffer so a read failure
    // wipes whatever had already been decrypted.
    let mut decrypted = Zeroizing::new(String::with_capacity(encrypted.len()));
    reader
        .read_to_string(&mut decrypted)
        .map_err(corrupt_body)?;
    Ok(std::mem::take(&mut *decrypted))
}

/// Wraps a body-read failure as [`CryptError::CorruptFile`], naming a payload
/// that is cryptographically intact but not UTF-8 separately from real damage.
fn corrupt_body(error: std::io::Error) -> CryptError {
    let message = if error.kind() == std::io::ErrorKind::InvalidData {
        "the decrypted file is not valid UTF-8 text"
    } else {
        "the ciphertext body is damaged"
    };
    CryptError::CorruptFile {
        message: message.to_string(),
        cause: Some(Box::new(error)),
    }
}

/// Sorts an eager age failure into the outcome a user can act on.
///
/// This runs before the body is read, so it only ever sees header problems and
/// the file-key unwrap; body damage is reported by [`decrypt`] itself.
fn classify(error: age::DecryptError) -> CryptError {
    use age::DecryptError;

    match error {
        DecryptError::DecryptionFailed
        | DecryptError::KeyDecryptionFailed
        | DecryptError::NoMatchingKeys => CryptError::WrongPassphrase,
        DecryptError::ExcessiveWork { required, .. } => CryptError::WorkFactorTooHigh {
            required,
            limit: MAX_WORK_FACTOR,
        },
        other => CryptError::CorruptFile {
            message: "the secret file is not a readable age file".to_string(),
            cause: Some(Box::new(other)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Offset of the scrypt salt inside the textual age header.
    fn salt_offset(encrypted: &[u8]) -> usize {
        let tag = b"-> scrypt ";
        let start = encrypted
            .windows(tag.len())
            .position(|window| window == tag.as_slice())
            .expect("every file this module writes carries a scrypt stanza");
        start + tag.len()
    }

    /// Rewrites the scrypt work factor (the decimal argument after the salt)
    /// without touching the rest of the header.
    fn with_work_factor(encrypted: &[u8], log_n: u8) -> Vec<u8> {
        let digits_start = salt_offset(encrypted) + 22 + 1;
        let digits_end = digits_start
            + encrypted[digits_start..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
        assert!(
            digits_end > digits_start,
            "the scrypt stanza must carry a decimal work factor"
        );
        let mut patched = Vec::with_capacity(encrypted.len());
        patched.extend_from_slice(&encrypted[..digits_start]);
        patched.extend_from_slice(log_n.to_string().as_bytes());
        patched.extend_from_slice(&encrypted[digits_end..]);
        patched
    }

    #[test]
    fn encrypting_then_decrypting_returns_the_text() {
        let plaintext = r#"{"name": "Test", "value": 42}"#;
        let passphrase = "my-strong-passphrase-123";

        let encrypted = encrypt(plaintext, passphrase).unwrap();
        let decrypted = decrypt(&encrypted, passphrase).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn an_empty_payload_round_trips() {
        let encrypted = encrypt("", "test-pass").unwrap();
        assert_eq!(decrypt(&encrypted, "test-pass").unwrap(), "");
    }

    #[test]
    fn the_output_starts_with_the_age_magic() {
        let encrypted = encrypt("test", "header-check").unwrap();
        assert!(encrypted.starts_with(b"age-encrypt"));
    }

    #[test]
    fn a_large_payload_round_trips() {
        let plaintext = "a".repeat(512 * 1024);
        let encrypted = encrypt(&plaintext, "large-data-test").unwrap();
        let decrypted = decrypt(&encrypted, "large-data-test").unwrap();
        assert_eq!(decrypted.len(), plaintext.len());
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn a_unicode_passphrase_round_trips() {
        let plaintext = "unicode content ✓";
        let passphrase = "пароль🔑";

        let encrypted = encrypt(plaintext, passphrase).unwrap();
        assert_eq!(decrypt(&encrypted, passphrase).unwrap(), plaintext);
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
            Err(CryptError::CorruptFile { .. })
        ));
    }

    #[test]
    fn a_truncated_body_is_reported_as_a_corrupt_file() {
        let encrypted = encrypt("secret", "right-pass").unwrap();
        let truncated = &encrypted[..encrypted.len() - 4];
        assert!(matches!(
            decrypt(truncated, "right-pass"),
            Err(CryptError::CorruptFile { .. })
        ));
    }

    #[test]
    fn a_structurally_damaged_header_is_reported_as_a_corrupt_file() {
        // Locate the salt rather than a hardcoded offset, so an age release
        // that shifts the layout cannot silently turn this into a body test.
        let mut encrypted = encrypt("secret", "right-pass").unwrap();
        let offset = salt_offset(&encrypted);
        encrypted[offset] = b'!'; // '!' is not base64
        assert!(matches!(
            decrypt(&encrypted, "right-pass"),
            Err(CryptError::CorruptFile { .. })
        ));
    }

    #[test]
    fn a_header_altered_in_a_base64_valid_way_reads_as_a_wrong_passphrase() {
        // A scrypt salt is authenticated only by the password-derived key, so
        // a still-well-formed salt edit is indistinguishable from a typo. The
        // test pins the honest behaviour the user-facing wording must cover.
        let mut encrypted = encrypt("secret", "right-pass").unwrap();
        let offset = salt_offset(&encrypted);
        encrypted[offset] = if encrypted[offset] == b'A' {
            b'B'
        } else {
            b'A'
        };
        assert!(matches!(
            decrypt(&encrypted, "right-pass"),
            Err(CryptError::WrongPassphrase)
        ));
    }

    #[test]
    fn a_file_above_the_work_factor_limit_is_reported_as_such() {
        // The check happens before scrypt runs, so a patched header costs no
        // work even though it claims a factor far above the limit.
        let encrypted = encrypt("secret", "right-pass").unwrap();
        let patched = with_work_factor(&encrypted, MAX_WORK_FACTOR + 20);
        match decrypt(&patched, "right-pass") {
            Err(CryptError::WorkFactorTooHigh { required, limit }) => {
                assert_eq!(required, MAX_WORK_FACTOR + 20);
                assert_eq!(limit, MAX_WORK_FACTOR);
            }
            other => panic!("expected WorkFactorTooHigh, got {other:?}"),
        }
    }

    #[test]
    fn empty_input_is_reported_as_a_corrupt_file() {
        assert!(matches!(
            decrypt(&[], "pass"),
            Err(CryptError::CorruptFile { .. })
        ));
    }

    #[test]
    fn non_age_input_is_reported_as_a_corrupt_file() {
        assert!(matches!(
            decrypt(b"not an age file at all", "pass"),
            Err(CryptError::CorruptFile { .. })
        ));
    }

    #[test]
    fn a_corrupt_file_error_exposes_its_cause() {
        let error = CryptError::CorruptFile {
            message: "the ciphertext body is damaged".to_string(),
            cause: Some(Box::new(std::io::Error::other("unexpected end of file"))),
        };
        assert!(error.to_string().starts_with("corrupt file: "));
        let source = error.source().expect("the cause is forwarded");
        assert_eq!(source.to_string(), "unexpected end of file");
    }

    #[test]
    fn a_wrong_passphrase_has_no_cause() {
        assert!(CryptError::WrongPassphrase.source().is_none());
    }
}
