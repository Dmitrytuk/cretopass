use std::collections::HashSet;

use zeroize::Zeroizing;

use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
};
use uuid::Uuid;

use super::node::{ExtraField, Node};
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
        /// Currently highlighted row index (0 = description, 1+ = extra fields).
        selected: usize,
        /// Copy confirmation alert text, if any.
        alert: Option<String>,
    },
    /// Inline form for editing an existing node's name/description/extra fields.
    Editing {
        node_id: Uuid,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
        field: usize,
        cursor: usize,
    },
    /// Inline form for adding a new child node.
    Adding {
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
        field: usize,
        cursor: usize,
    },
    /// Popup for selecting a field type to add (login, password, custom).
    SelectFieldType {
        node_id: Option<Uuid>,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
        field: usize,
        cursor: usize,
        selected_type: usize,
    },
    /// Popup for entering a custom field name.
    CustomFieldName {
        node_id: Option<Uuid>,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
        field: usize,
        cursor: usize,
        input: String,
    },
    /// Confirmation popup for deleting an extra field.
    ConfirmDeleteField {
        node_id: Option<Uuid>,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
        field: usize,
        cursor: usize,
        target_index: usize,
    },
    /// Confirmation popup for discarding unsaved changes in edit/add mode.
    ConfirmDiscardChanges {
        node_id: Option<Uuid>,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
        field: usize,
        cursor: usize,
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
/// `Nav` moves between fields and triggers form-level actions (save, add
/// field, cancel); `Input` edits the focused field's text. Entered with `i`,
/// left with `Esc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFocus {
    /// Navigation level: `j`/`k`/arrows/Tab move between fields.
    Nav,
    /// Input level: printable keys edit the focused field.
    Input,
}

/// Central application state.
#[derive(Debug, Clone)]
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
    /// Which level of the Add/Edit form has focus (`Nav` or `Input`).
    pub form_focus: FormFocus,
    pub quit: bool,
    pub return_to_menu: bool,
    /// Path of the currently opened secret file, if any.
    pub active_vault_path: Option<String>,
    /// Master password for the open vault (provided by user), zeroized on drop.
    pub active_vault_master_password: Zeroizing<Vec<u8>>,
    /// Set to `true` when the tree has unsaved changes; cleared after auto-save.
    pub modified: bool,
    /// If `true`, window title shows the full path; otherwise just the filename.
    pub display_full_path: bool,
    /// Status indicator shown in the window title bar (e.g. "Loading..." or "Saving...").
    pub status: Option<&'static str>,
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
            form_focus: FormFocus::Nav,
            quit: false,
            return_to_menu: false,
            active_vault_path: None,
            active_vault_master_password: Zeroizing::new(Vec::new()),
            modified: false,
            display_full_path: true,
            status: None,
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

    /// Inserts the cut buffer node as a sibling immediately after the selected node.
    pub fn paste_sibling(&mut self) {
        let node = match &self.cut_buffer {
            Some(n) => n.clone(),
            None => return,
        };
        let selected_id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        if self.root.contains(selected_id) {
            insert_after(&mut self.root, selected_id, node);
            self.update_visible();
            self.modified = true;
        }
    }

    /// Inserts the cut buffer node as a sibling immediately before the selected node.
    ///
    /// No-op when the selected node is the root (it has no parent to provide
    /// a sibling slot).
    pub fn paste_sibling_before(&mut self) {
        let node = match &self.cut_buffer {
            Some(n) => n.clone(),
            None => return,
        };
        let selected_id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        if insert_before(&mut self.root, selected_id, node) {
            self.update_visible();
            self.modified = true;
        }
    }

    /// Inserts the cut buffer node as the last child of the selected node.
    pub fn paste_child(&mut self) {
        let node = match &self.cut_buffer {
            Some(n) => n.clone(),
            None => return,
        };
        let selected_id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        if let Some(target) = self.root.find_mut(selected_id) {
            target.add_child(node);
            self.expanded.insert(selected_id);
            self.update_visible();
            self.modified = true;
        }
    }

    /// Finds a node by [`Uuid`] and replaces its name, description, and extra fields.
    pub fn update_node(
        &mut self,
        node_id: Uuid,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
    ) {
        if let Some(target) = self.root.find_mut(node_id) {
            target.name = name;
            target.description = desc;
            target.extra_fields = extra_fields;
            self.update_visible();
            self.modified = true;
        }
    }

    /// Creates a new child node under the currently selected node.
    pub fn add_child_to_selected(
        &mut self,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
    ) {
        let id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        let final_name = if name.is_empty() {
            texts::DEFAULT_NAME.to_string()
        } else {
            name
        };
        let mut node = Node::new(&final_name, &desc);
        node.extra_fields = extra_fields;
        if let Some(target) = self.root.find_mut(id) {
            target.add_child(node);
            self.expanded.insert(id);
            self.update_visible();
            self.modified = true;
        }
    }

    /// Creates a new sibling node immediately after the currently selected node.
    ///
    /// No-op when the selected node is the root (it has no parent to provide
    /// a sibling slot).
    pub fn add_sibling_to_selected(
        &mut self,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
    ) {
        let id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        let final_name = if name.is_empty() {
            texts::DEFAULT_NAME.to_string()
        } else {
            name
        };
        let mut node = Node::new(&final_name, &desc);
        node.extra_fields = extra_fields;
        if insert_after(&mut self.root, id, node) {
            self.update_visible();
            self.modified = true;
        }
    }

    /// Creates a new sibling node immediately before the currently selected node.
    ///
    /// No-op when the selected node is the root (it has no parent to provide
    /// a sibling slot).
    pub fn add_sibling_before_to_selected(
        &mut self,
        name: String,
        desc: String,
        extra_fields: Vec<ExtraField>,
    ) {
        let id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        let final_name = if name.is_empty() {
            texts::DEFAULT_NAME.to_string()
        } else {
            name
        };
        let mut node = Node::new(&final_name, &desc);
        node.extra_fields = extra_fields;
        if insert_before(&mut self.root, id, node) {
            self.update_visible();
            self.modified = true;
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

fn insert_after(root: &mut Node, target_id: Uuid, node: Node) -> bool {
    if let Some(parent) = find_parent_mut(root, target_id) {
        let pos = parent.children.iter().position(|c| c.id == target_id);
        if let Some(pos) = pos {
            parent.children.insert(pos + 1, node);
            return true;
        }
    }
    false
}

fn insert_before(root: &mut Node, target_id: Uuid, node: Node) -> bool {
    if let Some(parent) = find_parent_mut(root, target_id) {
        let pos = parent.children.iter().position(|c| c.id == target_id);
        if let Some(pos) = pos {
            parent.children.insert(pos, node);
            return true;
        }
    }
    false
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
        if let Some(s) = state.status {
            let el = match s {
                texts::STATUS_LOADING => ElementStyle::StatusLoading,
                texts::STATUS_SAVING => ElementStyle::StatusSaving,
                _ => ElementStyle::StatusInfo,
            };
            title_spans.push(Span::styled(format!(" {}  ", s), el.style()));
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
        let a1 = Node::new("A1", "");
        let a2 = Node::new("A2", "");
        let a = Node::with_children("A", "", vec![a1, a2]);
        let a_id = a.id;
        let b = Node::new("B", "");
        let root = Node::with_children("Secrets", "", vec![a, b]);
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
        state.cut_buffer = Some(Node::new("X", ""));
        state.paste_sibling_before();
        assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "X", "B"]);
    }

    #[test]
    fn add_sibling_before_to_selected_inserts_in_front() {
        let mut state = make_tree();
        state.selected = 4; // "B"
        state.add_sibling_before_to_selected("X".to_string(), String::new(), Vec::new());
        assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "X", "B"]);
    }

    #[test]
    fn add_sibling_before_is_noop_for_root() {
        let mut state = make_tree();
        state.selected = 0;
        state.add_sibling_before_to_selected("X".to_string(), String::new(), Vec::new());
        assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "B"]);
    }
}
