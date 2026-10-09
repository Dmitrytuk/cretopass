# Vault

Vault lifecycle: opening, creating, browsing for, configuring, and exchanging secret files.

## Language

**Home**: The launcher shown on startup. Lists Open secret file, Create new secret file, Configuration, Change file password, Export/Import actions, and Recent secret files.
_Avoid_: Settings menu, Launcher

**File Browser**: Modal for navigating the filesystem to pick a secret file (Open), name a new one (Create), or select a file for import.
_Avoid_: File Picker, Directory Browser

**Secret file**: An age-encrypted file containing a JSON-serialized Node tree. The primary persistence format. Identified by the `age-encryption.org/v1` header.
_Avoid_: Encrypted file, Vault file

**Master password**: The passphrase used to encrypt and decrypt a secret file via the age protocol. Distinct from Password ExtraFields stored inside the tree.
_Avoid_: Passphrase, Encryption key

**Active vault**: The currently open secret file and its state: file path, master password, and decrypted tree.
_Avoid_: Active file, Opened file

**Auto-save**: The automatic persistence mechanism. Every mutation to the tree marks it as Modified; on the next event loop tick, the tree is serialized, encrypted, and written to disk.
_Avoid_: Auto-persist

**Modified**: A flag set when the tree has unsaved changes. Triggers Auto-save at the next event loop iteration.
_Avoid_: Dirty, Unsaved

**Configuration**: Persistent app preferences stored as a binary file in the config directory. Contains Recent secret files, last opened vault path, display preferences, and auto-open-on-startup behavior.
_Avoid_: AppSettings, Preferences, App configuration

**Recent secret files**: A deduplicated, most-recent-first list of previously opened secret file paths. Displayed in Home below the main actions. Selecting one skips the File Browser and opens the master password prompt directly.
_Avoid_: Recent files, History

**Export**: Writing a Node (with or without children) to a plain-text file in JSON, XML, or CSV format alongside the active secret file.

**Import**: Reading a JSON, XML, or CSV file and merging the parsed Node tree as a child of the current Root. A timestamped wrapper node is created for multi-root imports.