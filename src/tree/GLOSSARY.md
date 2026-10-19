# Secrets Tree

The data model and workspace for organizing secrets in a hierarchical tree.

## Language

**Node**: The fundamental data unit. A named item that can carry ExtraFields and optionally contain child Nodes. Nodes with children are branch nodes; nodes without are leaf nodes.
_Avoid_: Entry, Item, Record

**Root**: The top-level Node of the tree. Cannot be cut or deleted. Every secret file has exactly one Root.

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

**Password**: A FieldType storing a password value. Masked as `*****` when displayed, shown in plain text only while actively editing.

**Custom(name)**: A FieldType for arbitrary user-defined data (URL, API key, notes, etc.). The `name` is the user-chosen label.

**Tree**: The main workspace where the node tree is navigated, searched, and edited. Entered from Home after opening a secret file.
_Avoid_: Tree view, Tree app, Workspace

**Detail**: Overlay listing a selected Node's ExtraFields. Rows can be navigated and copied to the system clipboard.
_Avoid_: Info panel, View mode

**Edit**: Live inline form for modifying an existing Node's name and ExtraFields. Nav level moves fields and triggers actions; Input level edits text. Leaving the Input level commits the edited field straight into the Node.
_Avoid_: Editing mode

**Add**: Live inline form for creating a new Node. Starts with the Name row alone; Description and ExtraFields are added via `+ Add Field`. The first committed change creates the Node at the pending position, after which the session behaves as Edit. An empty name defaults to "Untitled".
_Avoid_: Adding mode

**Form**: The shared Add/Edit state (`FormState`). Holds the Node being edited (or the Add draft), the ordered row list (`FormRow`), the focused row, the Nav/Input level, the field cut buffer, and the undo history.

**Draft**: The form-local Node holding an Add session's data before its first commit inserts it into the Tree. Discarded if the Add is cancelled; unused by Edit.

**Field cut buffer**: The form-local slot (`FormState::field_cut_buffer`) holding an ExtraField removed with `dd`, ready to be pasted with `p`/`P`. Reorders fields without touching the node until the form is saved; discarded with the form.
_Avoid_: Clipboard, Yank buffer

**Undo history**: The form-local record of a Node's successive committed states during one Edit session, capped by a redo tail. Cleared when the session ends. Distinct from the system clipboard and the Cut buffer.
_Avoid_: Change log, Version history

**Expand**: Revealing a Branch Node's children in the Tree view. Collapse hides them again.
_Avoid_: Open

**Collapse**: Hiding a Branch Node's children in the Tree view.

**Cut buffer**: The internal clipboard holding a cut Node, ready for paste. Copy-style: can paste multiple times. Distinct from the system clipboard.
_Avoid_: Clipboard, Yank buffer

**Search**: Filtering the visible tree by node name. Activated with `/`. Matched nodes are highlighted; navigation is constrained to matches while search is active.
_Avoid_: Filter, Find