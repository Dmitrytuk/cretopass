# cretopass

A terminal-based secret keeper. Organizes secrets in a hierarchical tree, stores them in age-encrypted secret files on disk, and auto-saves every change.

## Core data

**Node**: The fundamental data unit. A named, described item that can carry ExtraFields and optionally contain child Nodes. Nodes with children are branch nodes; nodes without are leaf nodes.
_Avoid_: Entry, Item, Record

**Root**: The top-level Node of the tree. Cannot be cut or deleted. Every secret file has exactly one Root.

**Branch**: A Node that has one or more children. Displays expand/collapse toggles.
_Avoid_: Folder, Category

**Leaf**: A Node with no children.
_Avoid_: Entry, Item, Endpoint

**ExtraField**: A key-value pair attached to a Node beyond its name and description. Three types: Login, Password, or Custom.
_Avoid_: Attribute, Property, Field

**FieldType**: The kind of an ExtraField — `Login`, `Password`, or `Custom(name)`.

**Login**: A FieldType storing a username or login credential.

**Password**: A FieldType storing a password value. Masked as `*****` when displayed, shown in plain text only while actively editing.

**Custom(name)**: A FieldType for arbitrary user-defined data (URL, API key, notes, etc.). The `name` is the user-chosen label.

## Application screens

**Home**: The launcher shown on startup. Lists Open secret file, Create new secret file, Configuration, Change file password, Export/Import actions, and Recent secret files.
_Avoid_: Settings menu, Launcher

**Tree**: The main workspace where the node tree is navigated, searched, and edited. Entered from Home after opening a secret file.
_Avoid_: Tree view, Tree app, Workspace

**File Browser**: Modal for navigating the filesystem to pick a secret file (Open), name a new one (Create), or select a file for import.
_Avoid_: File Picker, Directory Browser

**Detail**: Overlay showing a selected Node's description and ExtraFields. Rows can be navigated and copied to the system clipboard.
_Avoid_: Info panel, View mode

**Edit**: Inline form for modifying an existing Node's name, description, and ExtraFields.
_Avoid_: Editing mode

**Add**: Inline form for creating a new Node. Supports name, description, and ExtraFields. Empty name defaults to "Untitled".
_Avoid_: Adding mode

## Persistence

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

## Navigation and interaction

**Expand**: Revealing a Branch Node's children in the Tree view. Collapse hides them again.
_Avoid_: Open

**Collapse**: Hiding a Branch Node's children in the Tree view.

**Cut buffer**: The internal clipboard holding a cut Node, ready for paste. Copy-style: can paste multiple times. Distinct from the system clipboard.
_Avoid_: Clipboard, Yank buffer

**Search**: Filtering the visible tree by node name. Activated with `/`. Matched nodes are highlighted; navigation is constrained to matches while search is active.
_Avoid_: Filter, Find

## Configuration and history

**Configuration**: Persistent app preferences stored as a binary file in the config directory. Contains Recent secret files, last opened vault path, display preferences, and auto-open-on-startup behavior.
_Avoid_: AppSettings, Preferences, App configuration

**Recent secret files**: A deduplicated, most-recent-first list of previously opened secret file paths. Displayed in Home below the main actions. Selecting one skips the File Browser and opens the master password prompt directly.
_Avoid_: Recent files, History

## File exchange

**Export**: Writing a Node (with or without children) to a plain-text file in JSON, XML, or CSV format alongside the active secret file.

**Import**: Reading a JSON, XML, or CSV file and merging the parsed Node tree as a child of the current Root. A timestamped wrapper node is created for multi-root imports.