//! The Add/Edit form behind the Detail modal's Form Popup.
//!
//! Holds the form's own state machine ([`FormState`]), its undo/redo history
//! ([`Snapshot`]) and the scratch buffer typed at the Input level ([`Scratch`]),
//! plus the password-generator popup state ([`GeneratorModal`]).

use uuid::Uuid;

use crate::generator::{PasswordLengthError, PasswordParams};
use crate::tree::node::{ExtraField, FieldType, Node};

/// Which level of the two-level Add/Edit form currently has focus.
///
/// `Nav` moves between fields and triggers form-level actions (add field,
/// cut/paste, undo, close); `Input` edits the focused field's text. Entered
/// with `i` or `Enter`, left with `Enter` or `Esc` (which commits the field).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormFocus {
    /// Navigation level: `j`/`k`/arrows/Tab move between fields.
    Nav,
    /// Input level: printable keys edit the focused field.
    Input,
}

/// A single row of the Add/Edit form, in display order.
///
/// The concrete list of rows depends on the form kind; see [`FormState::rows`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormRow {
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
/// changing the open screen: the Form Popup stays put and only its `pending`
/// field changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormPending {
    /// No prefix pending.
    None,
    /// First `d` seen; the next `d` cuts the focused field.
    Cut,
    /// First `p` seen; `p`/`P` choose paste after/before.
    Paste,
}

/// A snapshot of a node's name and extra fields, used for form undo/redo.
#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    /// Node name at capture time.
    pub(crate) name: String,
    /// Extra fields at capture time.
    pub(crate) extra_fields: Vec<ExtraField>,
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
pub(crate) struct Scratch {
    /// The text being edited.
    pub(crate) text: String,
    /// Cursor byte-offset inside [`Scratch::text`].
    pub(crate) cursor: usize,
}

/// Editable state of the Add/Edit form.
///
/// For Edit (`node_id == Some`) the form holds no copy of the node's data: it
/// reads and writes the real [`Node`] in the tree, which is the single source
/// of truth. For Add (`node_id == None`) it holds a pre-creation `draft` node,
/// used until the first committed change inserts the node into the tree.
#[derive(Debug, Clone)]
pub(crate) struct FormState {
    /// `Some` = editing an existing node; `None` = adding a new node.
    pub(crate) node_id: Option<Uuid>,
    /// Pre-creation working node for the Add form; ignored once `node_id` is set.
    pub(crate) draft: Node,
    /// Index into the row list returned by [`FormState::rows`].
    pub(crate) selected_row: usize,
    /// Cursor byte-offset inside the focused field's text (Nav level).
    pub(crate) cursor: usize,
    /// Whether the form is in Nav or Input level.
    pub(crate) focus: FormFocus,
    /// Pending field cut/paste prefix, if any.
    pub(crate) pending: FormPending,
    /// Field removed by `dd`, waiting to be pasted.
    pub(crate) field_cut_buffer: Option<ExtraField>,
    /// Text of the focused field while at the Input level.
    pub(crate) scratch: Option<Scratch>,
    /// Undo/redo snapshots; `history[history_cursor]` is the current state.
    pub(crate) history: Vec<Snapshot>,
    /// Index of the current state within [`FormState::history`].
    pub(crate) history_cursor: usize,
    /// `true` when the top history entry was produced by a field generator.
    ///
    /// The next generation then replaces that entry instead of stacking a new
    /// one, so rerolling collapses into a single undo step. Every other commit
    /// and any undo/redo clears it.
    pub(crate) history_coalesce: bool,
}

impl FormState {
    /// Creates a form for adding a new node; only the name row is shown.
    pub(crate) fn new_add() -> Self {
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
            history_coalesce: false,
        }
    }

    /// Creates a form for editing the given node.
    pub(crate) fn new_edit(node: &Node) -> Self {
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
            history_coalesce: false,
        }
    }

    /// Returns `true` when this form has not yet created its node.
    pub(crate) fn is_adding(&self) -> bool {
        self.node_id.is_none()
    }

    /// Returns the node the form edits: the tree node for Edit, the draft for
    /// Add.
    ///
    /// # Panics
    ///
    /// Panics if the form edits a node that is not present in the tree; the
    /// owning handler keeps the edited node in the tree for the form's lifetime.
    pub(crate) fn node<'a>(&'a self, root: &'a Node) -> &'a Node {
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
    pub(crate) fn rows(&self, root: &Node) -> Vec<FormRow> {
        let count = self.node(root).extra_fields.len();
        let mut rows = Vec::with_capacity(count + 2);
        rows.push(FormRow::Name);
        rows.extend((0..count).map(FormRow::Extra));
        rows.push(FormRow::AddButton);
        rows
    }

    /// The currently focused row, clamped to the valid range.
    pub(crate) fn current_row(&self, root: &Node) -> FormRow {
        let rows = self.rows(root);
        rows[self.selected_row.min(rows.len() - 1)]
    }

    /// Keeps `selected_row` within bounds after the row list changes.
    pub(crate) fn clamp_selection(&mut self, root: &Node) {
        let len = self.rows(root).len();
        self.selected_row = self.selected_row.min(len.saturating_sub(1));
    }

    /// Selects the row at `index`, clamping to the valid range and resetting
    /// the cursor to the end of that row's text.
    pub(crate) fn select_row(&mut self, root: &Node, index: usize) {
        self.selected_row = index;
        self.clamp_selection(root);
        self.cursor = self.active_len(root);
    }

    /// The editable text of the focused row, if it has any.
    pub(crate) fn active_text<'a>(&'a self, root: &'a Node) -> Option<&'a str> {
        let node = self.node(root);
        match self.current_row(root) {
            FormRow::Name => Some(&node.name),
            FormRow::Extra(i) => node.extra_fields.get(i).map(|f| f.value.as_str()),
            FormRow::AddButton => None,
        }
    }

    /// Byte length of the focused row's text (0 for the add button).
    pub(crate) fn active_len(&self, root: &Node) -> usize {
        self.active_text(root).map_or(0, str::len)
    }

    /// The field type of the focused row, when that row is an extra field.
    pub(crate) fn focused_field_type<'a>(&'a self, root: &'a Node) -> Option<&'a FieldType> {
        let FormRow::Extra(index) = self.current_row(root) else {
            return None;
        };
        self.node(root)
            .extra_fields
            .get(index)
            .map(|field| &field.field_type)
    }

    /// Writes `value` into the focused extra field.
    ///
    /// Returns `false` when the focused row is not an extra field.
    pub(crate) fn set_focused_value(&mut self, root: &mut Node, value: String) -> bool {
        let FormRow::Extra(index) = self.current_row(root) else {
            return false;
        };
        let Some(field) = self.node_mut(root).extra_fields.get_mut(index) else {
            return false;
        };
        field.value = value;
        true
    }

    /// Enters the Input level, seeding the scratch buffer from the focused
    /// field. A no-op on the `+ Add Field` row (which has no text).
    pub(crate) fn begin_input(&mut self, root: &Node) {
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
    pub(crate) fn commit_input(&mut self, root: &mut Node) -> bool {
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
    pub(crate) fn cut_focused_field(&mut self, root: &mut Node) -> bool {
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
    pub(crate) fn paste_field_after(&mut self, root: &mut Node) -> bool {
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
    pub(crate) fn paste_field_before(&mut self, root: &mut Node) -> bool {
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
    pub(crate) fn push_field(&mut self, root: &mut Node, field: ExtraField) -> usize {
        let node = self.node_mut(root);
        node.extra_fields.push(field);
        node.extra_fields.len() - 1
    }

    /// Removes the extra field at `index`.
    ///
    /// Returns `true` when a field was removed.
    pub(crate) fn remove_field(&mut self, root: &mut Node, index: usize) -> bool {
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
    pub(crate) fn focus_new_field(&mut self, root: &Node, extra_index: usize) {
        self.selected_row = Self::row_index_of_extra(extra_index);
        self.clamp_selection(root);
        self.begin_input(root);
    }

    /// Resets the undo history to a single baseline snapshot of the form's node.
    pub(crate) fn seed_history(&mut self, root: &Node) {
        let snapshot = Snapshot::capture(self.node(root));
        self.history = vec![snapshot];
        self.history_cursor = 0;
        self.history_coalesce = false;
    }

    /// Records the form's node as a new history entry, discarding any redo tail.
    pub(crate) fn push_history(&mut self, root: &Node) {
        let snapshot = Snapshot::capture(self.node(root));
        self.history.truncate(self.history_cursor + 1);
        self.history.push(snapshot);
        self.history_cursor = self.history.len() - 1;
        self.history_coalesce = false;
    }

    /// Records a generated value, coalescing with the previous generation.
    ///
    /// When the top history entry was itself produced by a generation, it is
    /// replaced instead of pushed, so rerolling a value stays a single undo
    /// step. Any other commit, and any undo/redo, clears the flag and the next
    /// generation stacks normally.
    pub(crate) fn push_history_coalescing(&mut self, root: &Node) {
        if self.history_coalesce && !self.history.is_empty() {
            let snapshot = Snapshot::capture(self.node(root));
            self.history.truncate(self.history_cursor + 1);
            self.history[self.history_cursor] = snapshot;
        } else {
            self.push_history(root);
        }
        self.history_coalesce = true;
    }

    /// Returns `true` when an undo step is available.
    pub(crate) fn can_undo(&self) -> bool {
        self.history_cursor > 0
    }

    /// Returns `true` when a redo step is available.
    pub(crate) fn can_redo(&self) -> bool {
        self.history_cursor + 1 < self.history.len()
    }

    /// Moves one step back in history and applies the previous state.
    ///
    /// Returns `true` when the node changed.
    pub(crate) fn undo(&mut self, root: &mut Node) -> bool {
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
    pub(crate) fn redo(&mut self, root: &mut Node) -> bool {
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
        self.history_coalesce = false;
        self.clamp_selection(root);
        self.cursor = self.active_len(root);
    }
}

/// Transient state of the password-generator modal and its symbol checklist.
///
/// Shared by the Detail modal's generator popups (the parameter screen and the
/// symbol checklist) so the raw length buffer survives the round trip into the
/// checklist. The carried [`FormState`] is the same form the generator was
/// opened from.
#[derive(Debug, Clone)]
pub(crate) struct GeneratorModal {
    /// The Add/Edit form the generator writes into.
    pub(crate) form: FormState,
    /// Last valid parameters; updated only when a password is generated.
    pub(crate) params: PasswordParams,
    /// Raw length digits as typed; may be empty or out of range.
    pub(crate) length_input: String,
    /// Caret byte-offset inside [`GeneratorModal::length_input`].
    pub(crate) length_cursor: usize,
    /// `true` until the user edits the length: the first digit (or
    /// `Backspace`/`Delete`) replaces the seeded value instead of appending.
    pub(crate) length_fresh: bool,
    /// Why the current `length_input` cannot be used, set by Generate.
    pub(crate) error: Option<PasswordLengthError>,
    /// Focused row in the modal, or focused symbol index in the checklist.
    pub(crate) selected: usize,
}
