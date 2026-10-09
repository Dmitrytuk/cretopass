# Secrets Tree

The data model and workspace for organizing secrets in a hierarchical tree.

## Language

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

**Tree**: The main workspace where the node tree is navigated, searched, and edited. Entered from Home after opening a secret file.
_Avoid_: Tree view, Tree app, Workspace

**Detail**: Overlay showing a selected Node's description and ExtraFields. Rows can be navigated and copied to the system clipboard.
_Avoid_: Info panel, View mode

**Edit**: Inline form for modifying an existing Node's name, description, and ExtraFields.
_Avoid_: Editing mode

**Add**: Inline form for creating a new Node. Supports name, description, and ExtraFields. Empty name defaults to "Untitled".
_Avoid_: Adding mode

**Expand**: Revealing a Branch Node's children in the Tree view. Collapse hides them again.
_Avoid_: Open

**Collapse**: Hiding a Branch Node's children in the Tree view.

**Cut buffer**: The internal clipboard holding a cut Node, ready for paste. Copy-style: can paste multiple times. Distinct from the system clipboard.
_Avoid_: Clipboard, Yank buffer

**Search**: Filtering the visible tree by node name. Activated with `/`. Matched nodes are highlighted; navigation is constrained to matches while search is active.
_Avoid_: Filter, Find