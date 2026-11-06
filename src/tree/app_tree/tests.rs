//! Unit tests for the Tree event loop, grouped by concern.
//!
//! The shared helpers and the re-exports they need live here; each submodule
//! holds the tests for one concern and reaches the helpers through
//! `use super::*;`.

use super::input::*;
use super::r#loop::*;
use super::render::*;

use std::io;

use crossterm::event;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use tokio::sync::mpsc;
use zeroize::Zeroizing;

use crate::clipboard;
use crate::file_handling::{SaveError, SaveEvent};
use crate::generator;
use crate::theming::{Action, KeyRoute, route_key, texts};
use crate::tree::node::{ExtraField, FieldType, Node};
use crate::tree::widget::{
    AddPosition, Detail, ExportWizard, FormFocus, FormPending, FormRow, FormState,
    GENERATOR_ROW_GENERATE, GENERATOR_ROW_LENGTH, GENERATOR_ROW_SYMBOLS, GeneratorModal, Help,
    InputMode, Modal, PickerItem, SaveExit, SaveState, SharePassphrase, TreeHit, TreeState,
    form_hint, picker_items, render_extra_field_line,
};

mod actions;
mod autosave;
mod form;
mod hints;
mod mouse;
mod passwords;
mod paste;
mod render;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// The Detail modal `state` carries.
fn detail(state: &TreeState) -> &Detail {
    match &state.modal {
        Some(Modal::Detail(detail)) => detail.as_ref(),
        other => panic!("expected Detail modal, got {other:?}"),
    }
}

fn detail_mut(state: &mut TreeState) -> &mut Detail {
    match &mut state.modal {
        Some(Modal::Detail(detail)) => detail.as_mut(),
        other => panic!("expected Detail modal, got {other:?}"),
    }
}

/// Drives one key through the same routing the event loop uses: the active
/// modal first, then the navigation mode.
fn dispatch_key(key: KeyEvent, state: &mut TreeState) {
    if state.modal.is_some() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        let _ = super::input::handle_modal_key(key, state, &mut terminal);
    } else {
        super::input::dispatch_key(key, state);
    }
}

fn handle_form(key: KeyEvent, state: &mut TreeState) {
    dispatch_key(key, state);
}

/// Drives a Normal-mode key through the shared action dispatch, the path the
/// event loop uses with no modal open.
fn handle_normal(key: KeyEvent, state: &mut TreeState) {
    super::input::dispatch_action(Action::from_normal(&key), state);
}

fn editing_state() -> TreeState {
    let mut state = TreeState::new(Node::new("Root"));
    let node = state.root.clone();
    state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
    state
}

fn editing_state_with_fields() -> TreeState {
    let mut node = Node::new("Root");
    for value in ["a", "b", "c"] {
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: value.to_string(),
        });
    }
    let mut state = TreeState::new(node);
    let node = state.root.clone();
    state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
    state
}

fn editing_state_with_password(value: &str) -> TreeState {
    let mut node = Node::new("Root");
    node.extra_fields.push(ExtraField {
        field_type: FieldType::Password,
        value: value.to_string(),
    });
    let mut state = TreeState::new(node);
    let node = state.root.clone();
    state.modal = Some(Modal::Detail(Box::new(Detail::editing(&node))));
    state
}

fn form_of(state: &TreeState) -> &FormState {
    detail(state).form().expect("the form is open")
}

fn form_node(state: &TreeState) -> &Node {
    form_of(state).node(&state.root)
}

fn values(state: &TreeState) -> Vec<&str> {
    form_node(state)
        .extra_fields
        .iter()
        .map(|f| f.value.as_str())
        .collect()
}

fn select_row(state: &mut TreeState, index: usize) {
    let root = state.root.clone();
    detail_mut(state).form_mut().select_row(&root, index);
}

fn type_text(state: &mut TreeState, text: &str) {
    for c in text.chars() {
        dispatch_key(key(KeyCode::Char(c)), state);
    }
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

/// A state whose root "Root" has one expanded child "Child", so row 0 is
/// the Root and row 1 a node that has a parent.
fn state_with_a_child() -> TreeState {
    let root = Node::with_children("Root", vec![Node::new("Child")]);
    let root_id = root.id;
    let mut state = TreeState::new(root);
    state.expanded.insert(root_id);
    state.update_visible();
    state
}

/// Renders the whole Tree UI into a test backend and returns the visible
/// cells as text, one row per line, so a test can assert on a popup's
/// rendered output without matching across a row boundary.
fn rendered_text(state: &mut TreeState) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui(frame, state)).unwrap();
    let buffer = terminal.backend().buffer();
    let width = usize::from(buffer.area.width);
    buffer
        .content
        .chunks(width)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
