//! The Tree screen's model: the tree root, its flat visible list, and the
//! navigation, search and mutation operations over them.

use std::collections::HashSet;

use uuid::Uuid;

use super::modal::Modal;
use super::save::{SaveExit, SaveState};
use crate::file_browser::FileBrowserSession;
use crate::generator::PasswordParams;
use crate::theming::Spinner;
use crate::tree::node::Node;

/// A single visible row in the flattened tree display.
#[derive(Debug, Clone)]
pub(crate) struct VisibleNode {
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
}

/// Where a pending "Add" form should place the newly created node.
///
/// Chosen by the direction key inside `InputMode::PendingAdd` and consumed
/// when the form is confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddPosition {
    /// Insert immediately after the selected node.
    SiblingAfter,
    /// Insert immediately before the selected node.
    SiblingBefore,
    /// Append as the last child of the selected node.
    Child,
}

/// Central application state.
#[derive(Debug)]
pub struct TreeState {
    pub(crate) root: Node,
    pub(crate) expanded: HashSet<Uuid>,
    pub(crate) selected: usize,
    pub(crate) cut_buffer: Option<Node>,
    pub(crate) search_query: String,
    pub(crate) search_active: bool,
    pub(crate) visible: Vec<VisibleNode>,
    pub(crate) matched: HashSet<usize>,
    pub(crate) mode: InputMode,
    /// Where the pending "Add" form should place the new node.
    pub(crate) add_position: AddPosition,
    pub(crate) quit: bool,
    pub(crate) return_to_menu: bool,
    /// Path of the currently opened secret file, if any.
    pub(crate) active_vault_path: Option<String>,
    /// Directory and hidden-entry memory of the File Browser, seeded from Home
    /// and written back when the tree exits.
    pub(crate) browser_session: FileBrowserSession,
    /// Set to `true` when a commit has not yet been submitted to autosave.
    /// Cleared as soon as the snapshot is handed to the save worker.
    pub(crate) modified: bool,
    /// Persistence status of the autosave worker, shown in the title bar and as
    /// the save-error overlay.
    pub(crate) save_state: SaveState,
    /// Frame counter for the animated busy glyph shown next to `save_state`.
    pub(crate) spinner: Spinner,
    /// A requested exit that is parked waiting on the save worker.
    pub(crate) save_exit: SaveExit,
    /// If `true`, window title shows the full path; otherwise just the filename.
    pub(crate) display_full_path: bool,
    /// Password-generator settings remembered for this app run. Seeded from the
    /// home state and written back when the tree exits.
    pub(crate) generator: PasswordParams,
    /// The modal currently over the Tree, if any: at most one at a time. A key
    /// goes here while it is set, and to the [`InputMode`] navigation otherwise.
    pub(crate) modal: Option<Modal>,
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
            browser_session: FileBrowserSession::default(),
            modified: false,
            save_state: SaveState::Idle,
            spinner: Spinner::default(),
            save_exit: SaveExit::None,
            display_full_path: true,
            generator: PasswordParams::default(),
            modal: None,
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

    /// Returns `true` when the selected visible row is the Root.
    ///
    /// The Root is the only Node without a parent, so it has no sibling slot:
    /// it cannot be cut, and nothing can be added or pasted beside it.
    pub(crate) fn is_root_selected(&self) -> bool {
        self.visible
            .get(self.selected)
            .is_some_and(|row| row.id == self.root.id)
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
    pub(crate) fn insert_node_at_position(
        &mut self,
        position: AddPosition,
        node: Node,
    ) -> Result<Uuid, Node> {
        let Some(selected_id) = self.selected_node_id() else {
            return Err(node);
        };
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
            // `insert_after`/`insert_before` report when the target has no
            // sibling slot (the root, or an absent node) and hand the node back,
            // so it is never silently dropped.
            AddPosition::SiblingAfter => insert_after(&mut self.root, selected_id, node)?,
            AddPosition::SiblingBefore => insert_before(&mut self.root, selected_id, node)?,
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
    fn insert_node_at_position_hands_the_node_back_for_root_siblings() {
        for position in [AddPosition::SiblingBefore, AddPosition::SiblingAfter] {
            let mut state = make_tree();
            state.selected = 0;
            let node = Node::new("X");
            let Err(returned) = state.insert_node_at_position(position, node) else {
                panic!("a root has no sibling slot");
            };
            assert_eq!(returned.name, "X");
            assert_eq!(names(&state), vec!["Secrets", "A", "A1", "A2", "B"]);
            assert!(!state.modified);
        }
    }

    #[test]
    fn is_root_selected_is_true_for_the_root_row() {
        let mut state = make_tree();
        state.selected = 0;
        assert!(state.is_root_selected());
    }

    #[test]
    fn is_root_selected_is_false_off_the_root() {
        let mut state = make_tree();
        state.selected = 1; // "A"
        assert!(!state.is_root_selected());
    }
}
