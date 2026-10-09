# Glossary Map

## Contexts

- [Secrets Tree](./src/tree/GLOSSARY.md): the data model and workspace for organizing secrets in a hierarchical tree
- [Vault](./src/vault/GLOSSARY.md): vault lifecycle — opening, creating, browsing for, configuring, and exchanging secret files

## Relationships

- **Vault → Secrets Tree**: Vault opens a Secret file, decrypts it, and hands the resulting Node tree to the Tree workspace. The Tree auto-saves mutations back through the Vault's master password.