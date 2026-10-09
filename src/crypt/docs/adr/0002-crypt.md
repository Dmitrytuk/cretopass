# 0002 — crypt module

Thin wrapper around the `age` crate for passphrase-based encryption and
decryption. Exposes two functions: `encrypt(json, passphrase) → Vec<u8>` and
`decrypt(encrypted, passphrase) → String`.

**Decision:** use age (scrypt-backed passphrase encryption) rather than
NaCl/secretbox, GPG, or a custom scheme. Age files are portable — any
age-compatible tool (`rage`, `age` CLI) can decrypt them. The `age-encryption.org`
magic header in the binary format also lets `file_handling` detect encrypted
files without knowing the passphrase.

**Rejected alternative:** NaCl secretbox with a derived key. Would have required
storing a salt and nonce separately, and would not be interoperable with
existing tools.

## Contracts

- `encrypt()` wraps `age::Encryptor::with_user_passphrase()`.
- `decrypt()` uses `age::scrypt::Identity` for passphrase-derived identity.
- Passphrases are consumed as `&str`; the module internally wraps them in
  `age::secrecy::SecretString`.
- Both functions return `Box<dyn Error>` — callers in `file_handling` map this to
  `FileHandlingError::Crypt`.

This module has zero dependencies on other project modules. It is a pure leaf.

## Dependencies

- `age` 0.11 — encryption protocol