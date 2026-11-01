//! A vault passphrase, modelled as bytes.
//!
//! `age` accepts a passphrase only as `&str`, but the app carries the master
//! password as bytes — the autosave worker keeps the bytes it was handed. This
//! newtype is the one bridge between the two representations, so the non-UTF-8
//! check lives in one function rather than being spread across the facade.

use zeroize::Zeroizing;

use super::FileHandlingError;

/// A borrowed vault passphrase, as bytes.
///
/// Not to be confused with the share flow's `Passphrase`, which is the
/// password-editor prompt: this type is only ever the vault passphrase being
/// handed to the gateway.
///
/// The bytes are never shown: [`Debug`](std::fmt::Debug) redacts them, matching
/// the rest of the app's handling of the master password.
#[derive(Clone, Copy)]
pub struct Passphrase<'a>(&'a [u8]);

impl<'a> Passphrase<'a> {
    /// Borrows `bytes` as a passphrase.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }

    /// The raw bytes.
    pub fn as_bytes(&self) -> &'a [u8] {
        self.0
    }

    /// The passphrase as UTF-8, the form `age` accepts.
    ///
    /// # Errors
    ///
    /// Returns [`FileHandlingError::InvalidPassphraseEncoding`] when the bytes
    /// are not valid UTF-8.
    pub fn as_utf8(&self) -> Result<&'a str, FileHandlingError> {
        std::str::from_utf8(self.0).map_err(|_| FileHandlingError::InvalidPassphraseEncoding)
    }
}

impl std::fmt::Debug for Passphrase<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Passphrase").field(&"<redacted>").finish()
    }
}

impl<'a> From<&'a str> for Passphrase<'a> {
    fn from(value: &'a str) -> Self {
        Self(value.as_bytes())
    }
}

impl<'a> From<&'a String> for Passphrase<'a> {
    fn from(value: &'a String) -> Self {
        Self(value.as_bytes())
    }
}

impl<'a> From<&'a [u8]> for Passphrase<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self(value)
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for Passphrase<'a> {
    fn from(value: &'a [u8; N]) -> Self {
        Self(value)
    }
}

impl<'a> From<&'a Vec<u8>> for Passphrase<'a> {
    fn from(value: &'a Vec<u8>) -> Self {
        Self(value.as_slice())
    }
}

impl<'a> From<&'a Zeroizing<Vec<u8>>> for Passphrase<'a> {
    fn from(value: &'a Zeroizing<Vec<u8>>) -> Self {
        Self(value.as_slice())
    }
}

impl<'a> From<&'a Zeroizing<String>> for Passphrase<'a> {
    fn from(value: &'a Zeroizing<String>) -> Self {
        Self(value.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_borrow_shape_becomes_the_same_bytes() {
        let owned = String::from("hunter2");
        let bytes = b"hunter2";
        let vec = b"hunter2".to_vec();
        let zeroizing = Zeroizing::new(b"hunter2".to_vec());
        let zeroizing_string = Zeroizing::new(String::from("hunter2"));

        assert_eq!(Passphrase::from("hunter2").as_bytes(), b"hunter2");
        assert_eq!(Passphrase::from(&owned).as_bytes(), b"hunter2");
        assert_eq!(Passphrase::from(&bytes[..]).as_bytes(), b"hunter2");
        assert_eq!(Passphrase::from(bytes).as_bytes(), b"hunter2");
        assert_eq!(Passphrase::from(&vec).as_bytes(), b"hunter2");
        assert_eq!(Passphrase::from(&zeroizing).as_bytes(), b"hunter2");
        assert_eq!(Passphrase::from(&zeroizing_string).as_bytes(), b"hunter2");
        assert_eq!(Passphrase::new(b"hunter2").as_utf8().unwrap(), "hunter2");
    }

    #[test]
    fn non_utf8_bytes_are_rejected_as_utf8() {
        let passphrase = Passphrase::new(&[0xff, 0xfe]);

        assert!(matches!(
            passphrase.as_utf8(),
            Err(FileHandlingError::InvalidPassphraseEncoding)
        ));
    }

    #[test]
    fn debug_never_shows_the_bytes() {
        assert_eq!(
            format!("{:?}", Passphrase::new(b"hunter2")),
            "Passphrase(\"<redacted>\")"
        );
    }
}
