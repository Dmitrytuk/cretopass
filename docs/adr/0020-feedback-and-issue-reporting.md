# 0020 — Feedback and issue reporting

cretopass is an open-source terminal utility distributed through GitHub
Releases. Users need one obvious place to report bugs and request features, and
a **private** channel for security vulnerabilities so exploits are not
publicised before a patch ships.

**Decision:** ordinary bug reports and feature requests go to GitHub Issues
(`https://github.com/Dmitrytuk/cretopass/issues`) — the only feedback channel,
with no email address, chat, or Discord. Security vulnerabilities go through
GitHub's **Private Vulnerability Reporting** (`/security/advisories/new`),
enabled on the repository and documented in [`SECURITY.md`](../../SECURITY.md).
Issues are funnelled by GitHub Forms (`bug-report.yml`, `feature-request.yml`)
with required version/OS/steps fields, while `config.yml` disables blank issues
and points at the private security form.

Both channels, plus the app version and the MIT licence link, are surfaced in
one place in the app: the **About** overlay (`?` on Home, see
[ADR 0014](0014-status-bar-unification.md) for its badge) and the `--help`
footer. Their strings come from a single set of constants in `theming::texts`
(`APP_NAME`, `APP_VERSION`, `ISSUES_URL`, `SECURITY_ADVISORY_URL`,
`LICENSE_URL`, …), so the TUI and the CLI cannot drift apart. `cretopass
--version` stays a bare semver line, because `install.sh`/`install.ps1` parse
it.

**Rejected:** a dedicated `bug-report`/`doctor` CLI command and a feedback block
in the install scripts. The links in `--help`, the README, and the TUI already
cover discovery, and a probe command adds a system-inspection surface without
removing user effort. **Rejected:** OSC 8 terminal hyperlinks in the overlay —
ratatui does not account for their escape width and emulator support varies, so
links are printed as plain text.

Issue Forms only take effect once the templates reach the default branch
(`master`).

## Related ADRs

- [0004 — theming](../src/theming/docs/adr/0004-theming.md) — the module that
  holds the shared string constants and keybinding dispatch
- [0008 — CLI lifecycle commands](./0008-cli-lifecycle-commands.md) — the
  `remove` output where the rejected feedback links would have gone (the `update`
  half was later removed, see [0027](./0027-no-in-app-network-access.md))