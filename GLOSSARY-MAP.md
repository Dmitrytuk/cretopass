# Glossary Map

## Contexts

- [Secrets Tree](./src/tree/GLOSSARY.md): the data model and workspace for organizing secrets in a hierarchical tree
- [Vault](./src/vault/GLOSSARY.md): vault lifecycle — opening, creating, browsing for, configuring, and exchanging secret files
- [Cursor](./src/cursor/GLOSSARY.md): UTF-8-aware text editing primitives operating on byte offsets and TUI cursor field rendering

## Relationships

- **Vault → Secrets Tree**: Vault opens a Secret file, decrypts it, and hands the resulting Node tree to the Tree workspace. The Tree auto-saves mutations back through the Vault's master password.
- **Secrets Tree → Cursor**: Tree uses Cursor fields in Add and Edit forms for name, description, and ExtraField value editing.