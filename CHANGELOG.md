# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
