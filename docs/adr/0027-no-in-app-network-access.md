# 0027 — No in-app network access (removing self-update)

`cretopass` is a local password manager: it stores one age-encrypted file that
*you* choose the location of. Its security model promises "nothing is ever
transmitted over the network; the app does not use the network at all". The
`cretopass update` subcommand contradicted that promise: it downloaded
`install.sh` / `install.ps1` from GitHub Releases over HTTPS and **executed
it**, making the binary itself a network client and a remote-code-execution
path.

**Decision:** remove `cretopass update` entirely. The binary contains no
network code and makes no network requests in any mode. Updating is an
out-of-process, user-initiated action: re-run the published installer
(`curl … | sh` / `irm … | iex`) or use the system package manager.
`cretopass remove` is unchanged — it never touched the network.

## Why the old path was wrong

| Gap | Detail |
|---|---|
| Arbitrary software installation | `update` ran `sh install.sh` (Unix) or a detached `powershell -ExecutionPolicy Bypass -File install.ps1` (Windows) — remote shell code executed with the user's privileges, in the process that also holds decrypted secrets. |
| No authenticity check in the binary | [ADR 0008](0008-cli-lifecycle-commands.md) decision 7 admitted payload verification "is **not yet** implemented in Rust"; only the transport was pinned to HTTPS. |
| Fail-soft integrity | `install.sh` verifies a SHA-256 **fetched over the same channel** and, on a missing or mismatched hash, merely prompts "Continue anyway? [y/N]" — no signature, so no real authenticity. |
| Documentation contradiction | `README.md` claimed the app does not use the network while `update` did exactly that; `SECURITY.md` scope omitted the update/supply-chain channel. |

## Requirements from security sources

| Source | Requirement |
|---|---|
| KeePassXC (the closest offline-password-manager precedent) | Network access is never taken without explicit consent, and the whole networking stack is compile-time removable (`-DWITH_XC_NETWORKING=OFF`). |
| The Update Framework (TUF) | Any updater must defend against arbitrary software installation, rollback, freeze, mix-and-match, endless-data, and key-compromise attacks: compartmentalised trust, freshness, integrity of the whole repository view, and size limits. |
| NIST SSDF SP 800-218, PS.2 | Provide a mechanism for verifying software release integrity — cryptographic signatures, not a hash from the same untrusted channel. |
| OWASP Secrets Management Cheat Sheet | Minimise where secrets flow; keep the surface small. |
| Least privilege (already a project principle) | A local vault needs no egress; update the artifact out of the process that holds secrets. |

Taken together, a local password manager's correct network posture is **zero
requests by default**, with any network feature opt-in, separable, and
signature-verified. `update` satisfied none of these.

## Decisions

| # | Decision |
|---|---|
| 1 | Remove the `update` subcommand, `src/cli/update.rs`, and `LifecycleCommand::Update`. `cretopass update` is now an unknown argument. |
| 2 | The `cli` module (and the whole binary) performs no network I/O; no `curl` / `powershell` download path remains. |
| 3 | Updating is documented as an out-of-process action: re-run the installer or use the package manager. |
| 4 | `cretopass remove` and its layout mirroring are unchanged. |
| 5 | `README.md` and `SECURITY.md` state the no-network guarantee; the installer scripts are the update supply-chain channel and belong to the release process, not the runtime binary. |

## Consequences

- `LifecycleCommand` keeps only `Remove`; `main.rs` remains a pure parser +
  dispatcher ([ADR 0009](../src/cli/docs/adr/0009-cli-module.md)).
- `cli::layout`'s `TempDir` is now used at runtime only by the Windows
  self-remove helper; it stays cross-platform so its unit tests run everywhere
  (a documented `allow(dead_code)` covers non-Windows builds).
- Users lose the one-command convenience of `cretopass update`; `--help` and the
  README point at the installer instead.
- The binary's egress surface is now empty, matching the README claim.

## Supersedes

- [0008 — CLI lifecycle commands](0008-cli-lifecycle-commands.md): decisions
  2, 3, 5 (update half), and 7. The `remove` half stands.

## Rejected alternatives

- *Keep `update` but verify a detached signature (minisign/cosign).* Better than
  today, but still gives a secrets manager network egress and an in-process
  download + execute path; a signed updater is a large surface to build and
  audit.
- *Hide the network behind a Cargo feature (`self-update`, off by default).*
  Two behaviourally different binaries to test and document; any release that
  enables it reintroduces the problem.
- *A separate `cretopass-update` binary.* Keeps the manager clean, but adds a
  second distributed artifact and a new self-replace path for no user benefit
  over re-running the installer.
- *Keep `update` as a no-network adviser that prints the installer command.*
  Preserves discoverability without egress, but keeps a subcommand whose whole
  job is a printed hint; the README and `--help` already carry the command.
