# Secrets Tree

The data model and workspace for organizing secrets in a hierarchical tree.

## Language

**Node**: The fundamental data unit. A named item that can carry ExtraFields and optionally contain child Nodes. Nodes with children are branch nodes; nodes without are leaf nodes.
_Avoid_: Entry, Item, Record

**Root**: The top-level Node of the tree. It has no parent and therefore no siblings; it cannot be cut or deleted. It can still gain children. Every secret file has exactly one Root.

**Branch**: A Node that has one or more children. Displays expand/collapse toggles.
_Avoid_: Folder, Category

**Leaf**: A Node with no children.
_Avoid_: Entry, Item, Endpoint

**ExtraField**: A key-value pair attached to a Node beyond its name. Three types: Login, Password, or Custom.
_Avoid_: Attribute, Property, Field

**Description**: The conventional Custom ExtraField labelled "Description" holding a Node's free-text description. It is an ordinary ExtraField: addable, movable, duplicable and deletable like any other, and absent when the Node has no description. There is no dedicated Description field or type.
_Avoid_: Desc, Note

**FieldType**: The kind of an ExtraField — `Login`, `Password`, or `Custom(name)`.

**Login**: A FieldType storing a username or login credential.

**Login word**: The invented lowercase word the generator writes into a Login field. Assembled from syllables so it looks like a name (`brindel`, `quoravel`), never a persona, alias, or opaque string. It has no parameters: `Ctrl+G` generates one immediately and a second `Ctrl+G` rerolls it. Generation is a normal committed change; consecutive rerolls coalesce into one undo step.
_Avoid_: username generator, fake name

**Password**: A FieldType storing a password value. Masked as `*****` when displayed, shown in plain text only while actively editing.

**Generated password**: A Password value produced by the generator rather than typed. It is an ordinary committed change: undoable with `u`, masked like any other password, and autosaved. Generating over a non-empty value replaces it without confirmation, and consecutive generations coalesce into a single undo step.
_Avoid_: random password, auto-password

**Generator settings**: The last valid length and special-character selection used by the generator, remembered for the whole app run and never written to the configuration file. A fresh launch starts at 10 characters with all 17 symbols.
_Avoid_: generator config, password defaults

**Custom(name)**: A FieldType for arbitrary user-defined data (URL, API key, notes, etc.). The `name` is the user-chosen label.

**Tree**: The main workspace where the node tree is navigated, searched, and edited. Entered from Home after opening a secret file.
_Avoid_: Tree view, Tree app, Workspace

**Modal**: A window that takes over the keyboard until it is dismissed. At most one is open at a time, over the Tree's Normal navigation. Nothing outside it reacts to keys while it is up. Help, Detail, Alert, Export, and the File Browser are Modals.
_Avoid_: Overlay, Dialog, Window

**Popup**: A window nested inside a Modal — the Form inside Detail, or the field-type picker inside Form. It never outlives the Modal that opened it and returns to it when dismissed.
_Avoid_: Sub-dialog, Overlay

**Autosave**: The background writer that persists committed Tree changes. Each commit submits the current root; a newer submission replaces one that is still waiting, and `SaveState` shows a spinning glyph with `Saving` (or a static `Save failed`) in the title bar while the latest generation is tracked. Leaving the Tree waits for that generation for at most `SAVE_TIMEOUT`; if the final flush fails or times out, the user may retry or explicitly quit without saving (see `SaveExit`). The writer is stopped with a bounded `shutdown`, or detached on a force-quit.
_Avoid_: Auto-save, background save, sync

**Detail**: Modal listing a selected Node's ExtraFields. Rows can be navigated and copied to the system clipboard. Opens the Form Popup for editing.
_Avoid_: Info panel, View mode

**Edit**: Form Popup for modifying an existing Node's name and ExtraFields. Nav level moves fields and triggers actions; Input level edits text. Leaving the Input level commits the edited field straight into the Node.
_Avoid_: Editing mode

**Add**: Form Popup for creating a new Node. Starts with the Name row alone; Description and ExtraFields are added via `+ Add Field`. The first committed change creates the Node at the pending position, after which the session behaves as Edit. An empty name defaults to "Untitled".
_Avoid_: Adding mode

**Insertion position**: Where a newly added or pasted Node lands relative to the selected Node: after it, before it, or as its last child. After and before require the selected Node to have a parent; child works on any Node, including the Root.
_Avoid_: Paste target, slot

**Form**: The Add/Edit Popup inside Detail. Two levels: Nav moves between fields and triggers actions; Input edits the focused field's text.

**Draft**: The form-local Node holding an Add session's data before its first commit inserts it into the Tree. Discarded if the Add is cancelled; unused by Edit.

**Field cut buffer**: A holding slot for an ExtraField removed with `dd`, ready to be pasted with `p`/`P`. Discarded when the form closes.
_Avoid_: Clipboard, Yank buffer

**Undo history**: The form's record of successive committed states during one Edit session, capped by a redo tail. Consecutive generations collapse into one entry. Cleared when the form closes.
_Avoid_: Change log, Version history

**Expand**: Revealing a Branch Node's children in the Tree view. Collapse hides them again.
_Avoid_: Open

**Collapse**: Hiding a Branch Node's children in the Tree view.

**Cut buffer**: The internal clipboard holding a cut Node, ready for paste. Move-style: the next successful paste consumes the node, so it can be inserted only once. A failed paste leaves the node in the buffer. Distinct from the system clipboard.
_Avoid_: Clipboard, Yank buffer

**Help**: Modal opened with `?`. Lists the Tree's key bindings. Any key dismisses it.

**Alert**: Modal reporting an outcome — a successful export, a saved setting, a refused action. Displays a message; any key dismisses it.

**Export**: Writing a Node (with or without children) to a plain-text file. Three steps: pick the scope, pick the format (JSON, XML, or CSV), then name the destination in the File Browser. An export never overwrites a secret file.

**Share**: Writing the selected Node, with or without its children, (or the whole open file from Home) as a new **encrypted** secret file in a bundle the recipient opens with their own password, plus a plain-text instruction. Reached with `Ctrl+E` then `s`; what to share, the bundle folder and the password are chosen in three steps. Unlike an Export, nothing is written in the clear.
_Avoid_: Export (an Export is plain text)

**Search**: A navigation mode that filters the visible tree by node name. Activated with `/`. Matched nodes are highlighted; navigation is constrained to matches while search is active.
_Avoid_: Filter, Find