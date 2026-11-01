# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Share** a node (with or without its children) or the whole vault as a
  still-encrypted bundle the recipient opens in cretopass with their own
  password. Reached from the Tree with `Ctrl+E` then `s`, or from Home's "Share
  active file". The File Browser
  names the bundle folder, which gets a `<stem>.cretopass` file and a
  plain-text `HOW-TO-OPEN.txt`; the password is never written into the bundle.
- File browser can switch to mounted disks through a `Locations` overlay
  (`D`), toggle hidden dot-entries with `Ctrl+h` (or `Ctrl+Backspace`), and
  remember the last browsed directory and hidden setting for the rest of the
  session.
- The import file browser advertises the accepted formats (`json`/`csv`/`xml`)
  at the head of its status-bar hint.
- Home has an **About** overlay (press `?`): app name, version, tagline, the
  GitHub Issues link, the private security-reporting form, and the MIT license
  link. Any key dismisses it.
- `cretopass --help` now ends with the same app version, feedback links, and
  license. The strings are shared constants in `theming`, so the TUI overlay
  and the CLI help cannot drift apart. `cretopass --version` is unchanged.
- GitHub issue forms (bug report, feature request), `SECURITY.md`, and a README
  feedback section establish the project's feedback channel: ordinary reports
  go to GitHub Issues, security vulnerabilities through GitHub Private
  Vulnerability Reporting.

### Changed

- All screens now draw a single unified status bar (`[MODE] hints`) instead of a
  separate status line and help line. The Tree drops its `N items`/query context
  row; Home's static ` HOME ` label becomes a mode badge that reflects the open
  modal; the file browser no longer draws a status line of its own — the Home
  window renders its badge and hints (the `Hidden: on/off` and `Filter:`
  indicators are gone, with the `[PWD]` filter badge replacing the mode badge in
  every file-browser mode).
  Badge colours now encode the risk of the active action and mean the same thing
  in every screen.
- Modal windows no longer repeat their key hints inline: the Tree overlays, the
  Home modals, the File Browser name/rename/delete/error and `Locations` popups
  and the save-error / discard-confirm overlays all defer to the status bar.
  Because the File Browser draws no status bar, it routes its badge and hint
  through the owning Home window's status line.
- The file browser list and `Locations` overlay keep a 3-row margin around the
  cursor (`scrolloff`-style). The cursor moves within that margin before the
  viewport scrolls in either direction, instead of staying pinned to the bottom
  edge.
- Node descriptions are now ordinary extra fields (a `Custom` field labelled
  "Description") instead of a dedicated `description` slot. Descriptions can be
  added, reordered, duplicated and deleted like any other field, and a node has
  no description until one is added via `+ Add Field`. Existing vaults and
  exported XML/CSV/JSON files are migrated automatically on load: a non-empty
  legacy description becomes a leading Description field. The serialized format
  no longer contains a `description` key.
- The Home status-bar hint now lists `?:about`, so the new About overlay is
  discoverable from the menu.

### Fixed

- A cut node is now removed from the cut buffer after a successful paste, so it
  can no longer be pasted repeatedly. Previously each paste inserted a clone
  that kept the original node id, letting multiple nodes share one id.
- `p` in the tree no longer enters the paste prefix mode when the cut buffer is
  empty; the key is a no-op until something is cut.

## [0.1.0] - 2026-10-13

### Added

- Terminal UI for managing secrets with vim-style navigation (`h/j/k/l`, `gg`, `G`).
- Custom tree data structure for organising entries into folders and leaves.
- Search and filter across the tree.
- Edit and add forms for entries.
- In-memory editing with atomic autosave to an age-encrypted file.
- Home screen with multiple vaults and persisted configuration.
- File browser overlay for selecting vault and import/export paths.
- Theming layer with configurable styles, icons and strings.
- CLI subcommands: `cretopass update` (self-update via the installer) and
  `cretopass remove` (uninstall all artifacts).
- `install.sh` (Linux/macOS) and `install.ps1` (Windows) release installers
  with checksum verification, PATH integration, shell completions and GUI
  integration.
- Static musl builds for Linux, ad-hoc signed builds for macOS, and static-CRT
  builds for Windows (x86_64 and ARM64).
