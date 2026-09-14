use std::collections::HashSet;

use ratatui::{
    layout::Rect,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use uuid::Uuid;

use crate::theming::{ElementStyle, Icon, texts};
use super::node::{ExtraField, Node};

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
    /// Waiting for the second `d` to confirm a cut operation.
    AwaitingDd,
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
    Alert {
        message: String,
    },
}

/// Central application state.
#[derive(Debug, Clone)]
pub struct TreeState {
    pub root: Node,
    pub expanded: HashSet<Uuid>,
    pub selected: usize,
    pub clipboard: Option<Node>,
    pub search_query: String,
    pub search_active: bool,
    pub visible: Vec<VisibleNode>,
    pub matched: HashSet<usize>,
    pub mode: InputMode,
    pub quit: bool,
    pub return_to_menu: bool,
    /// Path of the currently opened encrypted file, if any.
    pub active_file_path: Option<String>,
    /// Passphrase for the open file (raw bytes, provided by user).
    pub active_file_passphrase: Vec<u8>,
    /// Set to `true` whenever the tree is mutated; cleared after save.
    pub dirty: bool,
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
            clipboard: None,
            search_query: String::new(),
            search_active: false,
            visible: Vec::new(),
            matched: HashSet::new(),
            mode: InputMode::Normal,
            quit: false,
            return_to_menu: false,
            active_file_path: None,
            active_file_passphrase: Vec::new(),
            dirty: false,
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

    /// Removes the selected node from the tree and stores it in the clipboard.
    pub fn cut_selected(&mut self) {
        let id = match self.selected_node_id() {
            Some(id) => id,
            None => return,
        };
        let node = remove_node(&mut self.root, id);
        if let Some(n) = node {
            self.clipboard = Some(n);
            self.update_visible();
            if self.selected >= self.visible.len() && !self.visible.is_empty() {
                self.selected = self.visible.len().saturating_sub(1);
            }
            self.dirty = true;
        }
    }

    /// Inserts the clipboard node as a sibling immediately after the selected node.
    pub fn paste_sibling(&mut self) {
        let node = match &self.clipboard {
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
            self.dirty = true;
        }
    }

    /// Inserts the clipboard node as the first child of the selected node.
    pub fn paste_child(&mut self) {
        let node = match &self.clipboard {
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
            self.dirty = true;
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
            self.dirty = true;
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
            self.dirty = true;
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
            .active_file_path
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
            title_spans.push(Span::styled(
                format!(" {}  ", s),
                el.style(),
            ));
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
