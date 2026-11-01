# 0002 — crypt module

Thin wrapper around the `age` crate for passphrase-based encryption and
decryption. Exposes two functions: `encrypt(plaintext, passphrase) → Vec<u8>`
and `decrypt(encrypted, passphrase) → String`.

**Decision:** use age (scrypt-backed passphrase encryption) rather than
NaCl/secretbox, GPG, or a custom scheme. Age files are portable — any
age-compatible tool (`rage`, `age` CLI) can decrypt them. The `age-encryption.org`
magic header in the binary format also lets `file_handling` detect encrypted
files without knowing the passphrase.

**Rejected alternative:** NaCl secretbox with a derived key. Would have required
storing a salt and nonce separately, and would not be interoperable with
existing tools.

## Contracts

- `encrypt()` wraps `age::Encryptor::with_user_passphrase()`; it accepts any
  UTF-8 text (in practice the JSON produced by `file_handling`).
- `decrypt()` uses `age::scrypt::Identity` for passphrase-derived identity.
- Passphrases are consumed as `&str`; the module internally wraps them in
  `age::secrecy::SecretString`.
- Both functions return `Result<_, CryptError>`. `CryptError` names the
  outcomes a user can act on — `WrongPassphrase`, `WorkFactorTooHigh`,
  `CorruptFile` and `EncryptionFailed`; callers in `file_handling` wrap it in
  `FileHandlingError::Crypt`. age unwraps the file key eagerly but verifies the
  ciphertext body lazily, so a wrong passphrase (eager unwrap failure) and a
  damaged body or header (lazy read or parse failure) end up in different
  variants. A header altered in a base64-valid way is indistinguishable from a
  wrong passphrase and is reported as such; the user-facing wording covers both.
- `CryptError` carries the underlying `age`/`std::io` error as a boxed
  `source()` so callers and loggers can print the full chain. It is
  `#[non_exhaustive]`, so a new variant is not a breaking change.

This module has zero dependencies on other project modules. It is a pure leaf.

## Work-factor policy

`decrypt` caps the accepted scrypt work factor with
`Identity::set_max_work_factor(MAX_WORK_FACTOR)`, where `MAX_WORK_FACTOR` is
`20`. Rationale:

- age tunes a file's work factor to roughly one second on the machine that
  creates it, and its default acceptance is `target + 4` — on a fast device
  that is ~16 s and several GiB of RAM for a hostile file, which an interactive
  UI cannot absorb.
- The reference age implementation caps acceptance at `22` (~15 s on a modern
  machine). cretopass opens untrusted files (share bundles, arbitrary `.age`
  files), so it accepts at most `20`, bounding a hostile file to
  `128 * 2^20 * 8` bytes (~1 GiB) and a few seconds of CPU.
- The check runs before scrypt, so an over-limit file costs no work and returns
  `WorkFactorTooHigh { required, limit }` — a distinct, actionable outcome that
  the Home screen words separately from "damaged".

**Accepted trade-off:** the limit is absolute, not `target + k`, because `age`
does not expose the device target. A device whose own target exceeds `20`
(unusually fast hardware) could create a file it then refuses to reopen; that
rare case gets the same clear, actionable error. Replacing the fixed ceiling
with a target-relative one remains future work.

## Secret hygiene

- Passphrases travel only as `SecretString` / `Zeroizing<Vec<u8>>` at the vault
  boundary.
- The partial plaintext buffer in `decrypt` is `Zeroizing`, so a failure
  mid-stream wipes whatever was already decrypted; the successful plaintext is
  handed to the caller, which owns it.
- **Known upstream gap:** `age` does not zeroize its derived scrypt key or work
  area. This module wraps the passphrase correctly; the gap is in the
  dependency and is recorded here rather than worked around.

## Public surface

`CryptError` is treated as an internal contract (`pub` only because the crate's
modules are `pub`). It is nonetheless `#[non_exhaustive]` with a boxed `source`
so it could be published without a breaking-change hazard. Distribution intent
is internal; if the crate is ever published, freeze the payload shape first.

## Dependencies

- `age` 0.11 — encryption protocol
- `zeroize` — partial-plaintext zeroization