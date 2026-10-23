# Glossary Map

## Contexts

- [cretopass](./GLOSSARY.md): cross-context vocabulary — the app's identity and the ways users reach the project
- [Secrets Tree](./src/tree/GLOSSARY.md): the data model and workspace for organizing secrets in a hierarchical tree
- [Vault](./src/vault/GLOSSARY.md): vault lifecycle — opening, creating, browsing for, configuring, and exchanging secret files
- [Cursor](./src/cursor/GLOSSARY.md): UTF-8-aware text editing primitives operating on byte offsets and TUI cursor field rendering
- [Theming](./src/theming/GLOSSARY.md): shared presentation layer — strings, styles, and the Status bar and Base background drawn by every screen

## Relationships

- **Vault → cretopass**: the Home **About overlay** and the CLI `--help` footer surface the **Feedback channel** and version from the same shared strings.
- **Vault → Secrets Tree**: Vault opens a Secret file, decrypts it, and hands the resulting Node tree to the Tree workspace. The Tree autosaves mutations in the background through the Vault's master password, and returns the final root when the user goes back to Home.
- **Secrets Tree → Cursor**: Tree uses Cursor fields in Add and Edit forms for name, description, and ExtraField value editing.
- **Vault / Secrets Tree / File Browser → Theming**: each screen maps its own mode to a `(label, style, hint)` triple and calls the shared Status bar renderer; the screens never draw status chrome themselves.