# Vault

Vault lifecycle: opening, creating, browsing for, configuring, and exchanging secret files.

## Language

**Home**: The launcher shown on startup. Lists Open secret file, Create new secret file, Configuration, Change file password, Export/Import actions, and Recent secret files.
_Avoid_: Settings menu, Launcher

**Modal**: A window that takes over the keyboard until it is dismissed. At most one is open at a time, over the Home menu or the Tree. Nothing outside it reacts to keys while it is up.
_Avoid_: Overlay, Dialog, Popup (a Popup is the nested case)

**Popup**: A window nested inside a Modal — the File Browser's Locations list, its name and rename prompts. It never outlives the Modal that opened it.
_Avoid_: Sub-dialog, Overlay

**Confirmation**: A Modal that asks yes or no before something surprising or hard to undo: dropping a Recent secret file, re-encrypting the open file, creating one with no Master password.
_Avoid_: Prompt, Warning

**About modal**: The Home Modal opened with `?`. Shows the app name, version, tagline, the Feedback channel link, the private security-reporting form, and the license. Dismissed by any key, like the Tree's help overlay.
_Avoid_: Help screen, Info dialog, About overlay

**File Browser**: Modal for navigating the filesystem to pick a secret file (Open), name a new one (Create), select a file for import, or name the destination of an export.
_Avoid_: File Picker, Directory Browser

**File Browser session**: In-memory state shared by successive File Browser openings during one program run: the Last directory and whether Hidden entries are shown. Never written to config or disk.
_Avoid_: Browser config, Persisted browser state

**Last directory**: The directory most recently displayed by the File Browser. The next opening starts there when it is still listable, otherwise it falls back to Home.
_Avoid_: Previous folder, Current path

**Hidden entry**: A filesystem entry whose name starts with `.`. Browsing shows or hides these entries with the Hidden-entry toggle; no other names are treated as hidden.
_Avoid_: Dotfile, Secret file (reserved for encrypted vault files)

**Location**: A selectable row in the File Browser's Locations popup: either the pinned Home shortcut or a Mounted disk.
_Avoid_: Bookmark, Sidebar item

**Mounted disk**: A filesystem volume reported by the OS and offered as a Location, identified by its mount point, filesystem type, and free/total space.
_Avoid_: Drive, Partition, Volume

**Secret file**: An age-encrypted file containing a JSON-serialized Node tree. The primary persistence format. Identified by the `age-encryption.org/v1` header.
_Avoid_: Encrypted file, Vault file

**Master password**: The passphrase used to encrypt and decrypt a secret file via the age protocol. Distinct from Password ExtraFields stored inside the tree.
_Avoid_: Encryption key. "Passphrase" is acceptable for the input field the user types into, never for the cryptography itself.

**Active vault**: The currently open secret file and its state: file path, master password, and decrypted tree.
_Avoid_: Active file, Opened file

**Auto-save**: The automatic persistence mechanism. Every mutation to the tree marks it as Modified; on the next event loop tick, the tree is serialized, encrypted, and written to disk.
_Avoid_: Auto-persist

**Modified**: A flag set when the tree has unsaved changes. Triggers Auto-save at the next event loop iteration.
_Avoid_: Dirty, Unsaved

**Busy indicator**: The animated glyph plus label shown in the title bar while a blocking operation runs. Loading and Saving are busy and spin; Save failed is not busy and shows a static label.
_Avoid_: Spinner, Throbber, Progress

**Configuration**: Persistent app preferences stored as a binary file in the config directory. Contains Recent secret files, last opened vault path, display preferences, the Active colour scheme, and auto-open-on-startup behavior.
_Avoid_: AppSettings, Preferences, App configuration

**Recent secret files**: A deduplicated, most-recent-first list of previously opened secret file paths. Displayed in Home below the main actions. Selecting one skips the File Browser and opens the master password prompt directly.
_Avoid_: Recent files, History

**Export**: Writing a Node (with or without children) to a plain-text file in JSON, XML, or CSV format. The File Browser names the destination: the directory it has open is where the file lands, and the file name is shown in the browser's path bar. An export never overwrites a secret file.
_Avoid_: Dump, Backup

**Import**: Reading a JSON, XML, or CSV file and merging the parsed Node tree as a child of the current Root. A timestamped wrapper node is created for multi-root imports.