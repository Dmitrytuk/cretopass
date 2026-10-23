# Security Policy

cretopass stores secrets in an `age`-encrypted file, so security reports are
taken seriously. Please **never** disclose a vulnerability in a public issue.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting:

👉 https://github.com/Dmitrytuk/cretopass/security/advisories/new

You can also reach it from the repository's **Security** tab →
**Report a vulnerability**. The report stays private between you and the
maintainer until a fix is released.

If you can, include:

- the affected version (`cretopass --version`),
- operating system and architecture,
- a description of the impact and, when possible, reproduction steps or a
  proof of concept.

## Supported versions

cretopass is pre-1.0 and distributed as a single binary. Only the latest
release is supported, so please reproduce against the newest version before
reporting.

## Scope

In scope:

- encryption and decryption of secret files (`age`),
- handling of the master password and other secrets in memory,
- the atomic save, import, and export paths,
- file-permission or privilege issues.

Out of scope:

- vulnerabilities in third-party dependencies — report those upstream,
- issues that require an already-compromised machine or terminal.