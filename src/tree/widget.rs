use std::collections::HashSet;

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use super::node::{ExtraField, Node};
use crate::file_handling::SaveError;
use crate::theming::{ElementStyle, Icon, texts};

/// A single visible row in the flattened tree display.
#[derive(Debug, Clone)]
pub struct VisibleNode {
    pub id: Uuid,
    pub name: String,
    pub depth: usize,
    pub is_parent: bool,
    pub expanded: bool,
}

/// Application input mode determining how key events are interpreted.
#[derive(Debug, Clone)]
pub enum InputMode {
    /// Default mode: navigation, expand/collapse, cut/paste, search.
    Normal,
    /// Search mode: typing a query, navigating between matches.
    Search,
    /// Waiting for the second `d` to confirm a cut operation (`PendingCut`).
    PendingCut,
    /// Waiting for the direction key after the first `p` (`PendingPaste`).
    PendingPaste,
    /// Waiting for the direction key after the first `a` (`PendingAdd`).
    PendingAdd,
    /// Detail overlay shown for the selected node.
    Detail {
        /// Currently highlighted extra-field row index.
        selected: usize,
        /// Copy confirmation alert text, if any.
        alert: Option<String>,
    },
    /// Add/Edit form. `node_id` distinguishes Edit (`Some`) from Add (`None`).
    Form(FormState),
    /// Popup for selecting a field type to add (login, password, custom,
    /// or description).
    SelectFieldType {
        form: FormState,
        /// Index into the currently available picker items.
        selected: usize,
    },
    /// Popup for entering a custom field name.
    CustomFieldName { form: FormState, input: String },
    /// Confirmation popup for deleting an extra field.
    ConfirmDeleteField {
        form: FormState,
        target_index: usize,
    },
    /// Popup for selecting export scope (node only or with children).
    ExportNodeSelect {
        /// The node being exported.
        node: Node,
        /// 0 = this node only, 1 = with children.
        selected: usize,
    },
    /// Popup for selecting export format (JSON/XML/CSV).
    ExportNodeFormat {
        /// The node being exported.
        node: Node,
        /// Whether children should be included.
        include_children: bool,
        /// 0 = JSON, 1 = XML, 2 = CSV.
        selected_format: usize,
    },
    /// Simple alert message popup (any key dismisses).
    Alert { message: String },
    /// Overlay showing the list of key bindings (any key dismisses).
    Help,
}

/// Where a pending "Add" form should place the newly created node.
///
/// Chosen by the direction key inside `InputMode::PendingAdd` and consumed
/// when the form is confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddPosition {
    /// Insert immediately after the selected node.
    SiblingAfter,
    /// Insert immediately before the selected node.
    SiblingBefore,
    /// Append as the last child of the selected node.
    Child,
}

/// Which level of the two-level Add/Edit form currently has focus.
///
/// `Nav` moves between fields and triggers form-level actions (add field,
/// cut/paste, undo, close); `Input` edits the focused field's text. Entered
/// with `i` or `Enter`, left with `Enter` or `Esc` (which commits the field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFocus {
    /// Navigation level: `j`/`k`/arrows/Tab move between fields.
    Nav,
    /// Input level: printable keys edit the focused field.
    Input,
}

/// A single row of the Add/Edit form, in display order.
///
/// The concrete list of rows depends on the form kind; see [`FormState::rows`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormRow {
    /// The node name (always present, never movable or deletable).
    Name,
    /// An extra field at the given index into the node's `extra_fields`.
    ///
    /// A node's description is itself an extra field (`Custom("Description")`),
    /// so it is edited, moved and deleted exactly like any other.
    Extra(usize),
    /// The `+ Add Field` button.
    AddButton,
}

/// A transient prefix state inside the form (field cut/paste).
///
/// Mirrors the tree's prefix modes (`PendingCut` / `PendingPaste`) without
/// introducing new `InputMode` variants: the form stays in `InputMode::Form`
/// and only its `pending` field changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormPending {
    /// No prefix pending.
    None,
    /// First `d` seen; the next `d` cuts the focused field.
    Cut,
    /// First `p` seen; `p`/`P` choose paste after/before.
    Paste,
}

/// A snapshot of a node's name and extra fields, used for form undo/redo.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Node name at capture time.
    pub name: String,
    /// Extra fields at capture time.
    pub extra_fields: Vec<ExtraField>,
}

impl Snapshot {
    /// Captures the current name and fields of `node`.
    fn capture(node: &Node) -> Self {
        Self {
            name: node.name.clone(),
            extra_fields: node.extra_fields.clone(),
        }
    }

    /// Writes this snapshot's data into `node`.
    fn apply(&self, node: &mut Node) {
        node.name.clone_from(&self.name);
        node.extra_fields.clone_from(&self.extra_fields);
    }
}

/// The text of the focused field while the form is at the Input level.
///
/// Typing edits this scratch copy; the node is only written when the Input
/// level is left.
#[derive(Debug, Clone)]
pub struct Scratch {
    /// The text being edited.
    pub text: String,
    /// Cursor byte-offset inside [`Scratch::text`].
    pub cursor: usize,
}

/// Editable state of the Add/Edit form.
///
/// For Edit (`node_id == Some`) the form holds no copy of the node's data: it
/// reads and writes the real [`Node`] in the tree, which is the single source
/// of truth. For Add (`node_id == None`) it holds a pre-creation `draft` node,
/// used until the first committed change inserts the node into the tree.
#[derive(Debug, Clone)]
pub struct FormState {
    /// `Some` = editing an existing node; `None` = adding a new node.
    pub node_id: Option<Uuid>,
    /// Pre-creation working node for the Add form; ignored once `node_id` is set.
    pub draft: Node,
    /// Index into the row list returned by [`FormState::rows`].
    pub selected_row: usize,
    /// Cursor byte-offset inside the focused field's text (Nav level).
    pub cursor: usize,
    /// Whether the form is in Nav or Input level.
    pub focus: FormFocus,
    /// Pending field cut/paste prefix, if any.
    pub pending: FormPending,
    /// Field removed by `dd`, waiting to be pasted.
    pub field_cut_buffer: Option<ExtraField>,
    /// Text of the focused field while at the Input level.
    pub scratch: Option<Scratch>,
    /// Undo/redo snapshots; `history[history_cursor]` is the current state.
    pub history: Vec<Snapshot>,
    /// Index of the current state within [`FormState::history`].
    pub history_cursor: usize,
}

impl FormState {
    /// Creates a form for adding a new node; only the name row is shown.
    pub fn new_add() -> Self {
        Self {
            node_id: None,
            draft: Node::new(String::new()),
            selected_row: 0,
            cursor: 0,
            focus: FormFocus::Nav,
            pending: FormPending::None,
            field_cut_buffer: None,
            scratch: None,
            history: Vec::new(),
            history_cursor: 0,
        }
    }

    /// Creates a form for editing the given node.
    pub fn new_edit(node: &Node) -> Self {
        Self {
            node_id: Some(node.id),
            draft: Node::new(String::new()),
            selected_row: 0,
            cursor: node.name.len(),
            focus: FormFocus::Nav,
            pending: FormPending::None,
            field_cut_buffer: None,
            scratch: None,
            history: vec![Snapshot::capture(node)],
            history_cursor: 0,
        }
    }

    /// Returns `true` when this form has not yet created its node.
    pub fn is_adding(&self) -> bool {
        self.node_id.is_none()
    }

    /// Returns the node the form edits: the tree node for Edit, the draft for
    /// Add.
    ///
    /// # Panics
    ///
    /// Panics if the form edits a node that is not present in the tree; the
    /// owning handler keeps the edited node in the tree for the form's lifetime.
    pub fn node<'a>(&'a self, root: &'a Node) -> &'a Node {
        match self.node_id {
            Some(id) => root.find(id).expect("edited node is present in the tree"),
            None => &self.draft,
        }
    }

    /// Returns a mutable reference to the node the form edits.
    ///
    /// # Panics
    ///
    /// Panics if the form edits a node that is not present in the tree; the
    /// owning handler keeps the edited node in the tree for the form's lifetime.
    fn node_mut<'a>(&'a mut self, root: &'a mut Node) -> &'a mut Node {
        match self.node_id {
            Some(id) => root
                .find_mut(id)
                .expect("edited node is present in the tree"),
            None => &mut self.draft,
        }
    }

    /// The ordered list of rows currently shown by this form.
    pub fn rows(&self, root: &Node) -> Vec<FormRow> {
        let count = self.node(root).extra_fields.len();
        let mut rows = Vec::with_capacity(count + 2);
        rows.push(FormRow::Name);
        rows.extend((0..count).map(FormRow::Extra));
        rows.push(FormRow::AddButton);
        rows
    }

    /// The currently focused row, clamped to the valid range.
    pub fn current_row(&self, root: &Node) -> FormRow {
        let rows = self.rows(root);
        rows[self.selected_row.min(rows.len() - 1)]
    }

    /// Keeps `selected_row` within bounds after the row list changes.
    pub fn clamp_selection(&mut self, root: &Node) {
        let len = self.rows(root).len();
        self.selected_row = self.selected_row.min(len.saturating_sub(1));
    }

    /// Selects the row at `index`, clamping to the valid range and resetting
    /// the cursor to the end of that row's text.
    pub fn select_row(&mut self, root: &Node, index: usize) {
        self.selected_row = index;
        self.clamp_selection(root);
        self.cursor = self.active_len(root);
    }

    /// The editable text of the focused row, if it has any.
    pub fn active_text<'a>(&'a self, root: &'a Node) -> Option<&'a str> {
        let node = self.node(root);
        match self.current_row(root) {
            FormRow::Name => Some(&node.name),
            FormRow::Extra(i) => node.extra_fields.get(i).map(|f| f.value.as_str()),
            FormRow::AddButton => None,
        }
    }

    /// Byte length of the focused row's text (0 for the add button).
    pub fn active_len(&self, root: &Node) -> usize {
        self.active_text(root).map_or(0, str::len)
    }

    /// Enters the Input level, seeding the scratch buffer from the focused
    /// field. A no-op on the `+ Add Field` row (which has no text).
    pub fn begin_input(&mut self, root: &Node) {
        let text = self.active_text(root).map(str::to_string);
        if let Some(text) = text {
            self.cursor = text.len();
            self.scratch = Some(Scratch {
                cursor: text.len(),
                text,
            });
            self.focus = FormFocus::Input;
        }
    }

    /// Leaves the Input level, committing the scratch text into the node when
    /// it differs from the field's current value.
    ///
    /// Returns `true` when the node was modified.
    pub fn commit_input(&mut self, root: &mut Node) -> bool {
        self.focus = FormFocus::Nav;
        let Some(scratch) = self.scratch.take() else {
            self.cursor = self.active_len(root);
            return false;
        };
        let changed = match self.current_row(root) {
            FormRow::Name => {
                if self.node(root).name == scratch.text {
                    false
                } else {
                    self.node_mut(root).name = scratch.text;
                    true
                }
            }
            FormRow::Extra(i) => {
                let current = self.node(root).extra_fields.get(i).map(|f| f.value.clone());
                match current {
                    Some(value) if value != scratch.text => {
                        self.node_mut(root).extra_fields[i].value = scratch.text;
                        true
                    }
                    _ => false,
                }
            }
            FormRow::AddButton => false,
        };
        self.cursor = self.active_len(root);
        changed
    }

    /// Cuts the focused extra field into `field_cut_buffer`.
    ///
    /// Returns `true` when a field was removed. No-op unless the focused row is
    /// an extra field.
    pub fn cut_focused_field(&mut self, root: &mut Node) -> bool {
        let FormRow::Extra(index) = self.current_row(root) else {
            return false;
        };
        let removed = {
            let node = self.node_mut(root);
            if index >= node.extra_fields.len() {
                return false;
            }
            node.extra_fields.remove(index)
        };
        self.field_cut_buffer = Some(removed);
        self.focus = FormFocus::Nav;
        self.clamp_selection(root);
        self.cursor = self.active_len(root);
        true
    }

    /// Pastes the cut field after the focused extra field.
    ///
    /// Returns `true` when a field was inserted. No-op unless the focused row is
    /// an extra field and a field is buffered.
    pub fn paste_field_after(&mut self, root: &mut Node) -> bool {
        let FormRow::Extra(index) = self.current_row(root) else {
            return false;
        };
        let Some(field) = self.field_cut_buffer.take() else {
            return false;
        };
        let at = {
            let node = self.node_mut(root);
            let at = (index + 1).min(node.extra_fields.len());
            node.extra_fields.insert(at, field);
            at
        };
        self.focus = FormFocus::Nav;
        self.selected_row = Self::row_index_of_extra(at);
        self.clamp_selection(root);
        self.cursor = self.active_len(root);
        true
    }

    /// Pastes the cut field before the focused extra field.
    ///
    /// Returns `true` when a field was inserted. No-op unless the focused row is
    /// an extra field and a field is buffered.
    pub fn paste_field_before(&mut self, root: &mut Node) -> bool {
        let FormRow::Extra(index) = self.current_row(root) else {
            return false;
        };
        let Some(field) = self.field_cut_buffer.take() else {
            return false;
        };
        self.node_mut(root).extra_fields.insert(index, field);
        self.focus = FormFocus::Nav;
        self.selected_row = Self::row_index_of_extra(index);
        self.clamp_selection(root);
        self.cursor = self.active_len(root);
        true
    }

    /// Index in [`FormState::rows`] of the `Extra(extra_index)` row.
    fn row_index_of_extra(extra_index: usize) -> usize {
        1 + extra_index
    }

    /// Appends `field` to the form's node and returns its extra-field index.
    pub fn push_field(&mut self, root: &mut Node, field: ExtraField) -> usize {
        let node = self.node_mut(root);
        node.extra_fields.push(field);
        node.extra_fields.len() - 1
    }

    /// Removes the extra field at `index`.
    ///
    /// Returns `true` when a field was removed.
    pub fn remove_field(&mut self, root: &mut Node, index: usize) -> bool {
        let node = self.node_mut(root);
        if index < node.extra_fields.len() {
            node.extra_fields.remove(index);
            true
        } else {
            false
        }
    }

    /// Moves the selection to the `Extra(extra_index)` row and enters the Input
    /// level on its (empty) value.
    pub fn focus_new_field(&mut self, root: &Node, extra_index: usize) {
        self.selected_row = Self::row_index_of_extra(extra_index);
        self.clamp_selection(root);
        self.begin_input(root);
    }

    /// Resets the undo history to a single baseline snapshot of the form's node.
    pub fn seed_history(&mut self, root: &Node) {
        let snapshot = Snapshot::capture(self.node(root));
        self.history = vec![snapshot];
        self.history_cursor = 0;
    }

    /// Records the form's node as a new history entry, discarding any redo tail.
    pub fn push_history(&mut self, root: &Node) {
        let snapshot = Snapshot::capture(self.node(root));
        self.history.truncate(self.history_cursor + 1);
        self.history.push(snapshot);
        self.history_cursor = self.history.len() - 1;
    }

    /// Returns `true` when an undo step is available.
    pub fn can_undo(&self) -> bool {
        self.history_cursor > 0
    }

    /// Returns `true` when a redo step is available.
    pub fn can_redo(&self) -> bool {
        self.history_cursor + 1 < self.history.len()
    }

    /// Moves one step back in history and applies the previous state.
    ///
    /// Returns `true` when the node changed.
    pub fn undo(&mut self, root: &mut Node) -> bool {
        if !self.can_undo() {
            return false;
        }
        self.history_cursor -= 1;
        let snapshot = self.history[self.history_cursor].clone();
        snapshot.apply(self.node_mut(root));
        self.after_history(root);
        true
    }

    /// Moves one step forward in history and re-applies the next state.
    ///
    /// Returns `true` when the node changed.
    pub fn redo(&mut self, root: &mut Node) -> bool {
        if !self.can_redo() {
            return false;
        }
        self.history_cursor += 1;
        let snapshot = self.history[self.history_cursor].clone();
        snapshot.apply(self.node_mut(root));
        self.after_history(root);
        true
    }

    /// Restores invariants after an undo/redo rewrote the node.
    ///
    /// The field cut buffer is cleared (it may reference fields the restored
    /// state no longer has), and the selection is clamped to the new row list.
    fn after_history(&mut self, root: &Node) {
        self.field_cut_buffer = None;
        self.clamp_selection(root);
        self.cursor = self.active_len(root);
    }
}

/// Persistence status of the Tree's background autosave.
///
/// Replaces the previous `status` / `save_error` pair, which could disagree. A
/// failed save keeps its [`SaveError`] until the next attempt, and tracks
/// whether the user has dismissed the error overlay.
#[derive(Debug)]
pub enum SaveState {
    /// No save is outstanding; the last attempt (if any) succeeded.
    Idle,
    /// The latest submitted snapshot is being written.
    Saving,
    /// The latest save attempt failed.
    Failed {
        /// The error reported by the save function.
        error: SaveError,
        /// Whether the error overlay has been dismissed.
        acknowledged: bool,
    },
}

impl SaveState {
    /// Title-bar label for this state, or `None` when there is nothing to show.
    pub fn title_label(&self) -> Option<&'static str> {
        match self {
            Self::Idle => None,
            Self::Saving => Some(texts::STATUS_SAVING),
            Self::Failed { .. } => Some(texts::STATUS_SAVE_FAILED),
        }
    }

    /// Title-bar style matching [`SaveState::title_label`].
    pub fn title_style(&self) -> Option<ElementStyle> {
        match self {
            Self::Idle => None,
            Self::Saving => Some(ElementStyle::StatusSaving),
            Self::Failed { .. } => Some(ElementStyle::StatusSaveFailed),
        }
    }

    /// Message for the save-error overlay, or `None` when it should not show.
    ///
    /// Returns `Some` only while a failure is unacknowledged; the first key
    /// dismisses it via [`SaveState::acknowledge`].
    pub fn overlay_message(&self) -> Option<String> {
        match self {
            Self::Failed {
                error,
                acknowledged: false,
            } => Some(format!("{}{}", texts::MSG_SAVE_FAILED, error)),
            Self::Idle | Self::Saving | Self::Failed { .. } => None,
        }
    }

    /// Marks a failure as acknowledged so its overlay stops showing.
    pub fn acknowledge(&mut self) {
        if let Self::Failed { acknowledged, .. } = self {
            *acknowledged = true;
        }
    }

    /// Whether the latest save attempt failed.
    pub fn has_failed(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// State of a requested exit that is waiting on the autosave worker.
///
/// A `q` / `H` / `Ctrl-C` request whose final flush fails parks here so the
/// user can retry, quit without saving, or cancel — instead of being trapped in
/// the Tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveExit {
    /// No exit is parked; the flush branch may run.
    None,
    /// The exit flush failed; the overlay offers retry / quit-anyway / stay.
    Blocked,
    /// The user chose to quit without saving; awaiting confirmation.
    ConfirmDiscard,
}

/// Central application state.
#[derive(Debug)]
pub struct TreeState {
    pub root: Node,
    pub expanded: HashSet<Uuid>,
    pub selected: usize,
    pub cut_buffer: Option<Node>,
    pub search_query: String,
    pub search_active: bool,
    pub visible: Vec<VisibleNode>,
    pub matched: HashSet<usize>,
    pub mode: InputMode,
    /// Where the pending "Add" form should place the new node.
    pub add_position: AddPosition,
    pub quit: bool,
    pub return_to_menu: bool,
    /// Path of the currently opened secret file, if any.
    pub active_vault_path: Option<String>,
    /// Set to `true` when a commit has not yet been submitted to autosave.
    /// Cleared as soon as the snapshot is handed to the save worker.
    pub modified: bool,
    /// Persistence status of the autosave worker, shown in the title bar and as
    /// the save-error overlay.
    pub save_state: SaveState,
    /// A requested exit that is parked waiting on the save worker.
    pub save_exit: SaveExit,
    /// If `true`, window title shows the full path; otherwise just the filename.
    pub display_full_path: bool,
}

impl TreeState {
    /// Creates a new `TreeState` with the given root.
    pub fn new(root: Node) -> Self {
        let mut state = Self {
            root,
            expanded: HashSet::new(),
            selected: 0,
            cut_buffer: None,
            search_query: String::new(),
            search_active: false,
            visible: Vec::new(),
            matched: HashSet::new(),
            mode: InputMode::Normal,
            add_position: AddPosition::Child,
            quit: false,
            return_to_menu: false,
            active_vault_path: None,
            modified: false,
            save_state: SaveState::Idle,
            save_exit: SaveExit::None,
            display_full_path: true,
        };
        state.update_visible();
        state
    }

    /// Rebuilds the `visible` flat list from the current expansion state,
    /// then recomputes search matches if a search is active.
    pub fn update_visible(&mut self) {
        self.visible = build_visible(&self.root, &self.expanded, 0);
        if self.search_active && !self.search_query.is_empty() {
            self.compute_matched();
        } else {
            self.matched.clear();
        }
    }

    fn compute_matched(&mut self) {
        self.matched.clear();
        let q = self.search_query.to_lowercase();
        for (i, v) in self.visible.iter().enumerate() {
            if v.name.to_lowercase().contains(&q) {
                self.matched.insert(i);
            }
        }
    }

    fn snap_to_first_match(&mut self) {
        if self.matched.is_empty() {
            return;
        }
        let mut sorted: Vec<usize> = self.matched.iter().copied().collect();
        sorted.sort_unstable();
        self.selected = sorted[0];
    }

    /// Moves selection to the next visible row.
    pub fn select_next(&mut self) {
        if self.search_active && !self.matched.is_empty() {
            let mut sorted: Vec<usize> = self.matched.iter().copied().collect();
            sorted.sort_unstable();
            if let Some(pos) = sorted.iter().position(|&i| i > self.selected) {
                self.selected = sorted[pos];
            } else {
                self.selected = sorted[0];
            }
        } else if !self.visible.is_empty() {
            self.selected = (self.selected + 1).min(self.visible.len() - 1);
        }
    }

    /// Moves selection to the previous visible row.
    pub fn select_prev(&mut self) {
        if self.search_active && !self.matched.is_empty() {
            let mut sorted: Vec<usize> = self.matched.iter().copied().collect();
            sorted.sort_unstable();
            if let Some(pos) = sorted.iter().rposition(|&i| i < self.selected) {
                self.selected = sorted[pos];
            } else {
                self.selected = sorted[sorted.len() - 1];
            }
        } else if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Returns the [`Uuid`] of the node at the current selection, if any.
    pub fn selected_node_id(&self) -> Option<Uuid> {
        self.visible.get(self.selected).map(|v| v.id)
    }

    /// Toggles the expanded/collapsed state of the selected node.
    pub fn toggle_expand(&mut self) {
        if let Some(id) = self.selected_node_id() {
            if self.expanded.contains(&id) {
                self.expanded.remove(&id);
            } else {
                self.expanded.insert(id);
            }
            self.update_visible();
        }
    }

    /// Explicitly expands or collapses the selected node.
    pub fn expand_collapse(&mut self, expand: bool) {
        if let Some(id) = self.selected_node_id() {
            if expand {
                self.expanded.insert(id);
            } else {
                self.expanded.remove(&id);
            }
            self.update_visible();
        }
    }

    /// Vim-style `h` behaviour: collapse the selected node if it is an expanded
    /// parent, otherwise jump the cursor to its parent.
    ///
    /// A no-op when the root row is selected (it has no parent).
    pub fn collapse_or_select_parent(&mut self) {
        let Some(current) = self.visible.get(self.selected) else {
            return;
        };
        if current.is_parent && current.expanded {
            let id = current.id;
            self.expanded.remove(&id);
            self.update_visible();
            return;
        }
        self.select_parent();
    }

    /// Moves the cursor to the parent row of the selected node, if any.
    ///
    /// Uses the `depth` stored on each visible row to find the closest
    /// preceding row with a smaller depth. A no-op on the root row.
    pub fn select_parent(&mut self) {
        let Some(current) = self.visible.get(self.selected) else {
            return;
        };
        if current.depth == 0 {
            return;
        }
        let target_depth = current.depth - 1;
        if let Some(parent_pos) = self.visible[..self.selected]
            .iter()
            .rposition(|v| v.depth == target_depth)
        {
            self.selected = parent_pos;
        }
    }

    /// Removes the selected node from the tree and stores it in the cut buffer.
    pub fn cut_selected(&mut self) {
        let id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        let node = remove_node(&mut self.root, id);
        if let Some(n) = node {
            self.cut_buffer = Some(n);
            self.update_visible();
            if self.selected >= self.visible.len() && !self.visible.is_empty() {
                self.selected = self.visible.len().saturating_sub(1);
            }
            self.modified = true;
        }
    }

    /// Inserts the cut buffer node as a sibling immediately after the selected
    /// node.
    ///
    /// The buffer is consumed on success (a cut node moves once); on failure
    /// the node is put back so it is not lost.
    pub fn paste_sibling(&mut self) {
        let Some(node) = self.cut_buffer.take() else {
            return;
        };
        let selected_id = match self.selected_node_id() {
            Some(id) => id,
            None => {
                self.cut_buffer = Some(node);
                return;
            }
        };
        match insert_after(&mut self.root, selected_id, node) {
            Ok(()) => {
                self.update_visible();
                self.modified = true;
            }
            Err(node) => self.cut_buffer = Some(node),
        }
    }

    /// Inserts the cut buffer node as a sibling immediately before the selected
    /// node.
    ///
    /// The buffer is consumed on success (a cut node moves once); on failure
    /// the node is put back so it is not lost. No-op when the selected node is
    /// the root (it has no parent to provide a sibling slot).
    pub fn paste_sibling_before(&mut self) {
        let Some(node) = self.cut_buffer.take() else {
            return;
        };
        let selected_id = match self.selected_node_id() {
            Some(id) => id,
            None => {
                self.cut_buffer = Some(node);
                return;
            }
        };
        match insert_before(&mut self.root, selected_id, node) {
            Ok(()) => {
                self.update_visible();
                self.modified = true;
            }
            Err(node) => self.cut_buffer = Some(node),
        }
    }

    /// Inserts the cut buffer node as the last child of the selected node.
    ///
    /// The buffer is consumed on success (a cut node moves once); on failure
    /// the node is put back so it is not lost.
    pub fn paste_child(&mut self) {
        let Some(node) = self.cut_buffer.take() else {
            return;
        };
        let selected_id = match self.selected_node_id() {
            Some(id) => id,
            None => {
                self.cut_buffer = Some(node);
                return;
            }
        };
        if let Some(target) = self.root.find_mut(selected_id) {
            target.add_child(node);
            self.expanded.insert(selected_id);
            self.update_visible();
            self.modified = true;
        } else {
            self.cut_buffer = Some(node);
        }
    }

    /// Inserts `node` relative to the currently selected node according to
    /// `position`, expands the parent, selects the new node, and returns its id.
    ///
    /// Returns `Err(node)` with the node unconsumed when the position is
    /// unavailable (a sibling slot for the root) or nothing is selected.
    pub fn insert_node_at_position(
        &mut self,
        position: AddPosition,
        node: Node,
    ) -> Result<Uuid, Node> {
        let Some(selected_id) = self.selected_node_id() else {
            return Err(node);
        };
        let has_sibling_slot = has_parent(&self.root, selected_id);
        if matches!(
            position,
            AddPosition::SiblingAfter | AddPosition::SiblingBefore
        ) && !has_sibling_slot
        {
            return Err(node);
        }
        let new_id = node.id;
        match position {
            AddPosition::Child => {
                if let Some(target) = self.root.find_mut(selected_id) {
                    target.add_child(node);
                } else {
                    return Err(node);
                }
                self.expanded.insert(selected_id);
            }
            AddPosition::SiblingAfter => {
                let _ = insert_after(&mut self.root, selected_id, node);
            }
            AddPosition::SiblingBefore => {
                let _ = insert_before(&mut self.root, selected_id, node);
            }
        }
        self.update_visible();
        self.select_by_id(new_id);
        self.modified = true;
        Ok(new_id)
    }

    /// Moves the selection to the visible row with the given [`Uuid`], if any.
    pub fn select_by_id(&mut self, id: Uuid) {
        if let Some(pos) = self.visible.iter().position(|v| v.id == id) {
            self.selected = pos;
        }
    }

    /// Returns the number of visible rows in the current tree view.
    pub fn num_visible(&self) -> usize {
        self.visible.len()
    }

    /// Returns the number of visible rows that match the active search query.
    pub fn num_matched(&self) -> usize {
        self.matched.len()
    }

    /// Returns the name of the currently selected visible row, if any.
    pub fn current_name(&self) -> Option<String> {
        self.visible.get(self.selected).map(|v| v.name.clone())
    }

    /// Clones and returns the full [`Node`] for the currently selected item.
    pub fn current_node(&self) -> Option<Node> {
        let id = self.visible.get(self.selected)?.id;
        find_node(&self.root, id)
    }

    /// Returns a reference to the [`Node`] for the currently selected item.
    pub fn current_node_ref(&self) -> Option<&Node> {
        let id = self.visible.get(self.selected)?.id;
        find_node_ref(&self.root, id)
    }

    /// Activates search for the given query string.
    pub fn set_search(&mut self, query: String) {
        self.search_active = true;
        if !query.is_empty() {
            expand_for_search(&self.root, &mut self.expanded, &query.to_lowercase());
        }
        self.search_query = query;
        self.update_visible();
        self.snap_to_first_match();
    }

    /// Deactivates search and clears all match highlights.
    pub fn clear_search(&mut self) {
        self.search_query.clear();
        self.search_active = false;
        self.matched.clear();
    }
}

fn find_node(root: &Node, id: Uuid) -> Option<Node> {
    if root.id == id {
        return Some(root.clone());
    }
    for child in &root.children {
        if let Some(found) = find_node(child, id) {
            return Some(found);
        }
    }
    None
}

fn find_node_ref(root: &Node, id: Uuid) -> Option<&Node> {
    if root.id == id {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_node_ref(child, id) {
            return Some(found);
        }
    }
    None
}

fn expand_for_search(node: &Node, expanded: &mut HashSet<Uuid>, query: &str) -> bool {
    let has_match = node.name.to_lowercase().contains(query);
    let mut child_has_match = false;
    for child in &node.children {
        if expand_for_search(child, expanded, query) {
            child_has_match = true;
        }
    }
    if (has_match || child_has_match) && !node.children.is_empty() {
        expanded.insert(node.id);
    }
    has_match || child_has_match
}

fn build_visible(node: &Node, expanded: &HashSet<Uuid>, depth: usize) -> Vec<VisibleNode> {
    let mut result = Vec::new();
    let is_parent = !node.is_leaf();
    let is_expanded = expanded.contains(&node.id);
    result.push(VisibleNode {
        id: node.id,
        name: node.name.clone(),
        depth,
        is_parent,
        expanded: is_expanded,
    });
    if is_expanded {
        for child in &node.children {
            result.extend(build_visible(child, expanded, depth + 1));
        }
    }
    result
}

fn remove_node(root: &mut Node, id: Uuid) -> Option<Node> {
    if let Some(pos) = root.children.iter().position(|c| c.id == id) {
        return Some(root.children.remove(pos));
    }
    for child in &mut root.children {
        if let Some(node) = remove_node(child, id) {
            return Some(node);
        }
    }
    None
}

/// Inserts `node` immediately after the child with `target_id`.
///
/// Returns the node back in `Err` when no parent of `target_id` exists (i.e.
/// the target is the root or is absent), so the caller can restore it.
fn insert_after(root: &mut Node, target_id: Uuid, node: Node) -> Result<(), Node> {
    if let Some(parent) = find_parent_mut(root, target_id) {
        let pos = parent.children.iter().position(|c| c.id == target_id);
        if let Some(pos) = pos {
            parent.children.insert(pos + 1, node);
            return Ok(());
        }
    }
    Err(node)
}

/// Inserts `node` immediately before the child with `target_id`.
///
/// Returns the node back in `Err` when no parent of `target_id` exists (i.e.
/// the target is the root or is absent), so the caller can restore it.
fn insert_before(root: &mut Node, target_id: Uuid, node: Node) -> Result<(), Node> {
    if let Some(parent) = find_parent_mut(root, target_id) {
        let pos = parent.children.iter().position(|c| c.id == target_id);
        if let Some(pos) = pos {
            parent.children.insert(pos, node);
            return Ok(());
        }
    }
    Err(node)
}

/// Returns `true` when some node in the tree has `id` as one of its children
/// (i.e. `id` is not the root).
fn has_parent(root: &Node, id: Uuid) -> bool {
    if root.children.iter().any(|c| c.id == id) {
        return true;
    }
    root.children.iter().any(|c| has_parent(c, id))
}

fn find_parent_mut(root: &mut Node, id: Uuid) -> Option<&mut Node> {
    if root.children.iter().any(|c| c.id == id) {
        return Some(root);
    }
    for child in &mut root.children {
        if let Some(parent) = find_parent_mut(child, id) {
            return Some(parent);
        }
    }
    None
}

/// Zero-sized type serving as a namespace for the tree rendering function.
pub struct TreeWidget;

impl TreeWidget {
    pub fn render(frame: &mut Frame, area: Rect, state: &TreeState) {
        let base_title = state
            .active_vault_path
            .as_deref()
            .map(|p| {
                if state.display_full_path {
                    format!("{} {} ", crate::theming::texts::TITLE_TREE_PREFIX, p)
                } else {
                    let name = std::path::Path::new(p)
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or(std::borrow::Cow::Borrowed(p));
                    format!("{} {} ", crate::theming::texts::TITLE_TREE_PREFIX, name)
                }
            })
            .unwrap_or_else(|| crate::theming::texts::TITLE_TREE_DEFAULT.to_string());

        let mut title_spans: Vec<Span> = Vec::new();
        if let (Some(label), Some(style)) = (
            state.save_state.title_label(),
            state.save_state.title_style(),
        ) {
            title_spans.push(Span::styled(format!(" {label}  "), style.style()));
        }
        title_spans.push(Span::raw(base_title));

        let block = Block::default()
            .title(Line::from(title_spans))
            .borders(Borders::ALL)
            .border_style(ElementStyle::TreeBorder.style());

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines: Vec<Line> = state
            .visible
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let indent = "  ".repeat(v.depth);
                let icon = if v.is_parent {
                    if v.expanded {
                        Icon::Expanded.as_str()
                    } else {
                        Icon::Collapsed.as_str()
                    }
                } else {
                    Icon::LeafPad.as_str()
                };
                let prefix = if i == state.selected {
                    Icon::SelectionArrow.as_str()
                } else {
                    Icon::SelectionPad.as_str()
                };
                let text = format!("{}{}{}{}", prefix, indent, icon, v.name);

                let is_matched = state.matched.contains(&i);
                let style = if is_matched {
                    ElementStyle::TreeRowMatched.style()
                } else {
                    ElementStyle::TreeRow.style()
                };
                Line::from(Span::styled(text, style))
            })
            .collect();

        let text = Text::from(lines);
        let inner_height = inner.height.max(1) as usize;
        let scroll_row = state
            .selected
            .saturating_sub(inner_height.saturating_sub(1) / 2);
        let paragraph = Paragraph::new(text)
            .style(ElementStyle::TreeRow.style())
            .scroll((scroll_row as u16, 0));
        frame.render_widget(paragraph, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Root "Secrets" with one expanded branch "A" (children A1, A2) and a leaf "B".
    fn make_tree() -> TreeState {
        let a1 = Node::new("A1");
        let a2 = Node::new("A2");
        let a = Node::with_children("A", vec![a1, a2]);
        let a_id = a.id;
        let b = Node::new("B");
        let root = Node::with_children("Secrets", vec![a, b]);
        let root_id = root.id;
        let mut state = TreeState::new(root);
        state.expanded.insert(root_id);
        state.expanded.insert(a_id);
        state.update_visible();
        state
    }

    fn names(state: &TreeState) -> Vec<String> {
        state.visible.iter().map(|v| v.name.clone()).collect()
    }

    #[test]
    fn collapse_or_select_parent_collapses_expanded_parent() {
        let mut state = make_tree();
        state.selected = 1; // "A", expanded parent
        state.collapse_or_select_parent();
        assert_eq!(names(&state), vec!["Secrets", "A", "B"]);
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn collapse_or_select_parent_moves_leaf_to_parent() {
        let mut state = make_tree();
        state.selected = 2; // "A1", leaf
        state.collapse_or_select_parent();
        assert_eq!(state.selected, 1); // parent "A"
    }

    #[test]
    fn collapse_or_select_parent_jumps_from_collapsed_parent() {
        let mut state = make_tree();
        state.expanded.remove(&state.visible[1].id);
        state.update_visible();
        state.selected = 1; // "A", collapsed parent
        state.collapse_or_select_parent();
        assert_eq!(state.selected, 0); // parent "Secrets"
    }

    #[test]
    fn collapse_or_select_parent_is_noop_on_root() {
        let mut state = make_tree();
        state.selected = 0;
        state.collapse_or_select_parent();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn paste_sibling_before_inserts_in_front() {
        let mut state = make_tree();
        state.selected = 4; // "B"
        state.cut_buffer = Some(Node::new("X"));
        state.paste_sibling_before();
        assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "X", "B"]);
    }

    #[test]
    fn paste_sibling_consumes_the_buffer() {
        let mut state = make_tree();
        state.selected = 1; // "A"
        state.cut_buffer = Some(Node::new("X"));
        state.paste_sibling();
        assert!(state.cut_buffer.is_none());
    }

    #[test]
    fn paste_child_consumes_the_buffer() {
        let mut state = make_tree();
        state.selected = 1; // "A"
        state.cut_buffer = Some(Node::new("X"));
        state.paste_child();
        assert!(state.cut_buffer.is_none());
    }

    #[test]
    fn second_paste_is_a_noop() {
        let mut state = make_tree();
        state.selected = 1; // "A"
        let cut = Node::new("X");
        let cut_id = cut.id;
        state.cut_buffer = Some(cut);
        state.paste_sibling();
        let after_first = names(&state);
        state.paste_sibling();
        assert_eq!(names(&state), after_first);
        assert_eq!(
            state
                .root
                .children
                .iter()
                .filter(|c| c.id == cut_id)
                .count(),
            1
        );
    }

    #[test]
    fn paste_sibling_before_root_keeps_the_buffer() {
        let mut state = make_tree();
        state.selected = 0; // root has no sibling slot
        state.cut_buffer = Some(Node::new("X"));
        state.paste_sibling_before();
        assert!(state.cut_buffer.is_some());
        assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "B"]);
    }

    #[test]
    fn insert_node_at_position_inserts_sibling_before() {
        let mut state = make_tree();
        state.selected = 4; // "B"
        let node = Node::new("X");
        let id = state
            .insert_node_at_position(AddPosition::SiblingBefore, node)
            .expect("sibling slot available");
        assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "X", "B"]);
        assert_eq!(state.visible[state.selected].id, id);
    }

    #[test]
    fn insert_node_at_position_is_noop_for_root_sibling() {
        let mut state = make_tree();
        state.selected = 0;
        let node = Node::new("X");
        let err = state.insert_node_at_position(AddPosition::SiblingBefore, node);
        assert!(err.is_err());
        assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "B"]);
    }
}
