use std::borrow::Cow;
use std::io::{self, Write};

use crate::cursor::{
    backspace_at, cursor_field_spans, cursor_left, cursor_right, delete_at, insert_char_at,
    selected_span,
};
use crate::file_handling::{Autosave, SAVE_TIMEOUT, SaveError, SaveEvent, save_tree};
use crate::generator::{self, PasswordLength, PasswordParams};
use crate::theming::Action;
use crate::theming::{
    ElementStyle, Icon, SPINNER_TICK, paint_background, render_status_bar, texts,
};
use crate::tree::node::{ExtraField, FieldType, Node};
use crate::tree::widget::{
    AddPosition, FormFocus, FormPending, FormRow, FormState, GeneratorModal, Help, InputMode,
    Modal, SaveExit, SaveState, TreeState, TreeWidget, execute,
};
use crossterm::event::{self, Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures_util::StreamExt;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::{Terminal, backend::Backend};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use zeroize::Zeroizing;

/// The vault location and credential the autosave worker needs to persist a
/// tree.
///
/// `path` and `passphrase` are always supplied together, so "path present,
/// passphrase absent" cannot be represented. A tree without a save target (the
/// read-only library demo) runs without autosave.
pub struct SaveTarget {
    /// Path of the secret file to write.
    pub path: String,
    /// Master password used to encrypt the file.
    pub passphrase: Zeroizing<Vec<u8>>,
}

impl std::fmt::Debug for SaveTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveTarget")
            .field("path", &self.path)
            .field("passphrase", &"<redacted>")
            .finish()
    }
}

/// Outcome of a completed Tree session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitStatus {
    /// The latest snapshot is on disk (or there was no autosave worker).
    Saved,
    /// The user chose to leave without persisting the latest changes.
    Unsaved,
}

/// Runs the tree screen until the user quits or returns to the home screen.
///
/// Committed changes are handed to a background save worker built from
/// `target`; `None` means a read-only tree (the library demo) with no autosave.
/// On exit the function waits for the latest snapshot to be written. A failed
/// flush parks an exit prompt so the user can retry, quit without saving (after
/// confirmation), or stay; the returned [`ExitStatus`] reports which happened.
///
/// # Errors
///
/// Returns an I/O error when the terminal or event stream fails, or when the
/// save worker cannot be started.
pub fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
    target: Option<SaveTarget>,
) -> io::Result<ExitStatus> {
    let runtime = save_runtime()?;
    let (save_tx, mut save_rx) = mpsc::unbounded_channel::<SaveEvent>();
    let mut autosave = start_autosave(target, save_tx)?;
    let has_worker = autosave.is_some();

    let mut submitted: u64 = 0;
    let mut saved: u64 = 0;
    let mut save_open = has_worker;

    let status = runtime.block_on(async {
        let mut events = EventStream::new();
        terminal.draw(|f| ui(f, state))?;

        // Drives the title-bar spinner. The first tick is immediate, but the
        // tick branch below is guarded by `save_state.spins()`, so the app does
        // not redraw at the spinner rate while nothing is outstanding.
        let mut tick = tokio::time::interval(SPINNER_TICK);
        tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                maybe_event = events.next() => {
                    match maybe_event {
                        Some(Ok(Event::Key(key))) => {
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            if state.save_exit != SaveExit::None
                                || state.save_state.overlay_message().is_some()
                            {
                                match handle_save_overlay(key, state) {
                                    OverlayAction::Discard => break Ok(ExitStatus::Unsaved),
                                    // Retry: fall through so the exit branch re-runs.
                                    OverlayAction::Retry => {}
                                    OverlayAction::Stay => {
                                        terminal.draw(|f| ui(f, state))?;
                                        continue;
                                    }
                                }
                            } else {
                                if state.modal.is_some() {
                                    handle_modal_key(key, state)?;
                                } else {
                                    dispatch_key(key, state);
                                }

                                if state.modified {
                                    state.modified = false;
                                    if let Some(ref mut worker) = autosave {
                                        submitted = worker.submit(state.root.clone());
                                        state.save_state = SaveState::Saving;
                                    }
                                }
                            }
                        }
                        Some(Ok(Event::Resize(_, _))) => {}
                        Some(Ok(_)) => continue,
                        Some(Err(e)) => return Err(e),
                        None => {
                            return Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "terminal event stream ended",
                            ));
                        }
                    }
                }
                event = save_rx.recv(), if save_open => {
                    match event {
                        Some(event) => {
                            if event.result.is_ok() {
                                saved = saved.max(event.generation);
                            }
                            if event.generation == submitted {
                                state.save_state = match event.result {
                                    Ok(()) => SaveState::Idle,
                                    Err(error) => SaveState::Failed {
                                        error,
                                        acknowledged: false,
                                    },
                                };
                            }
                        }
                        None => {
                            save_open = false;
                            if saved < submitted {
                                state.save_state = SaveState::Failed {
                                    error: SaveError::WorkerStopped,
                                    acknowledged: false,
                                };
                            }
                        }
                    }
                }
                _ = tick.tick(), if state.save_state.spins() => {
                    state.spinner.advance();
                }
            }

            if state.save_exit == SaveExit::None && (state.quit || state.return_to_menu) {
                if let Some(ref mut worker) = autosave {
                    let target =
                        match exit_plan(state.modified, &state.save_state, saved, submitted) {
                            ExitPlan::UpToDate => break Ok(ExitStatus::Saved),
                            ExitPlan::Wait(generation) => generation,
                            ExitPlan::ResubmitThenWait => {
                                state.modified = false;
                                worker.submit(state.root.clone())
                            }
                        };

                    state.save_state = SaveState::Saving;
                    terminal.draw(|f| ui(f, state))?;

                    let flush = tokio::time::timeout(
                        SAVE_TIMEOUT,
                        wait_for_save(terminal, state, &mut tick, &mut save_rx, &mut saved, target),
                    )
                    .await;
                    match flush {
                        Ok(result) => match result? {
                            Ok(()) => break Ok(ExitStatus::Saved),
                            Err(error) => park_blocked(state, error),
                        },
                        // The worker is still stuck; offer retry / force-quit.
                        Err(_elapsed) => park_blocked(state, SaveError::WorkerStopped),
                    }
                } else {
                    break Ok(ExitStatus::Saved);
                }
            }

            terminal.draw(|f| ui(f, state))?;
        }
    })?;

    if let Some(worker) = autosave {
        match status {
            // A force-quit may leave a save stuck; detach instead of waiting.
            ExitStatus::Unsaved => worker.detach(),
            ExitStatus::Saved => {
                let _ = worker.shutdown(SAVE_TIMEOUT);
            }
        }
    }

    Ok(status)
}

/// Builds the current-thread runtime that drives the event loop.
///
/// The time driver must be enabled explicitly (it is off by default); the
/// bounded exit flush relies on [`tokio::time::timeout`].
///
/// # Errors
///
/// Returns an error when the runtime cannot be built.
fn save_runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
}

/// What the exit path must do to make the vault durable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitPlan {
    /// Everything submitted has already been written.
    UpToDate,
    /// Wait for the snapshot with this generation.
    Wait(u64),
    /// Re-submit the current root, then wait for it.
    ResubmitThenWait,
}

/// Decides how an exit request should make the vault durable.
fn exit_plan(modified: bool, save_state: &SaveState, saved: u64, submitted: u64) -> ExitPlan {
    if modified || save_state.has_failed() {
        ExitPlan::ResubmitThenWait
    } else if saved < submitted {
        ExitPlan::Wait(submitted)
    } else {
        ExitPlan::UpToDate
    }
}

/// What the save-error overlay did with a key press.
enum OverlayAction {
    /// The key was consumed; stay in the Tree.
    Stay,
    /// Re-attempt the exit flush on the next loop iteration.
    Retry,
    /// Leave the Tree without persisting the latest changes.
    Discard,
}

/// Routes a key press while the save-error overlay is showing.
fn handle_save_overlay(key: event::KeyEvent, state: &mut TreeState) -> OverlayAction {
    match state.save_exit {
        SaveExit::Blocked => {
            let quit_requested = key.code == KeyCode::Char('q')
                || (key.modifiers.contains(KeyModifiers::CONTROL)
                    && key.code == KeyCode::Char('c'));
            if key.code == KeyCode::Char('r') || key.code == KeyCode::Enter {
                state.save_exit = SaveExit::None;
                OverlayAction::Retry
            } else if quit_requested {
                state.save_exit = SaveExit::ConfirmDiscard;
                OverlayAction::Stay
            } else {
                cancel_exit(state);
                OverlayAction::Stay
            }
        }
        SaveExit::ConfirmDiscard => {
            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                OverlayAction::Discard
            } else {
                cancel_exit(state);
                OverlayAction::Stay
            }
        }
        SaveExit::None => {
            state.save_state.acknowledge();
            OverlayAction::Stay
        }
    }
}

/// Cancels a parked exit: stay in the Tree and dismiss the overlay.
fn cancel_exit(state: &mut TreeState) {
    state.quit = false;
    state.return_to_menu = false;
    state.save_exit = SaveExit::None;
    state.save_state.acknowledge();
}

/// Parks a failed exit flush behind the retry / quit / stay overlay.
fn park_blocked(state: &mut TreeState, error: SaveError) {
    state.save_exit = SaveExit::Blocked;
    state.save_state = SaveState::Failed {
        error,
        acknowledged: false,
    };
}

/// Builds the background writer when the tree has a save target.
fn start_autosave(
    target: Option<SaveTarget>,
    events: mpsc::UnboundedSender<SaveEvent>,
) -> io::Result<Option<Autosave<Node>>> {
    let Some(SaveTarget { path, passphrase }) = target else {
        return Ok(None);
    };

    let save = move |root: &Node| save_tree(root, &path, &passphrase).map_err(SaveError::Failed);
    let on_event = move |event| {
        // The receiver is dropped when the tree screen closes; a late save
        // notification is then expected and intentionally ignored.
        let _ = events.send(event);
    };

    Autosave::spawn(save, on_event).map(Some)
}

/// Waits until the snapshot with `target` generation has been attempted,
/// animating the title-bar spinner while it waits.
///
/// The inner [`Result`] is the save outcome (as in [`wait_for_generation`]);
/// the outer one reports terminal I/O errors from redrawing the spinner, which
/// are kept distinct from a failed save.
async fn wait_for_save<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &mut TreeState,
    tick: &mut tokio::time::Interval,
    save_rx: &mut mpsc::UnboundedReceiver<SaveEvent>,
    saved: &mut u64,
    target: u64,
) -> io::Result<Result<(), SaveError>> {
    let wait = wait_for_generation(save_rx, saved, target);
    tokio::pin!(wait);
    loop {
        // Read before the `select!` so the guard does not borrow `state` while
        // the spinner branch mutates it.
        let spinner_on = state.save_state.spins();
        tokio::select! {
            result = &mut wait => return Ok(result),
            _ = tick.tick(), if spinner_on => {
                state.spinner.advance();
                terminal.draw(|f| ui(f, state))?;
            }
        }
    }
}

/// Waits until the snapshot with `target` generation has been attempted.
///
/// Returns the result of that attempt. The caller owns the `SaveState`
/// transition: on success the exit path is finished, so there is nothing left to
/// update. This is the pure core of [`wait_for_save`].
async fn wait_for_generation(
    save_rx: &mut mpsc::UnboundedReceiver<SaveEvent>,
    saved: &mut u64,
    target: u64,
) -> Result<(), SaveError> {
    while let Some(event) = save_rx.recv().await {
        if event.result.is_ok() {
            *saved = (*saved).max(event.generation);
        }
        if event.generation == target {
            return event.result;
        }
    }
    Err(SaveError::WorkerStopped)
}

/// Dispatches a key event to the handler for the current input mode.
fn dispatch_key(key: event::KeyEvent, state: &mut TreeState) {
    match &state.mode {
        InputMode::Normal => handle_normal(key, state),
        InputMode::Search => handle_search(key, state),
        InputMode::PendingCut => handle_pending_cut(key, state),
        InputMode::PendingPaste => handle_pending_paste(key, state),
        InputMode::PendingAdd => handle_pending_add(key, state),
        InputMode::Form(_) => handle_form(key, state),
        InputMode::SelectFieldType { .. } => handle_select_field_type(key, state),
        InputMode::CustomFieldName { .. } => handle_custom_field_name(key, state),
        InputMode::GeneratePassword(_) => handle_generate_password(key, state),
        InputMode::SymbolChecklist(_) => handle_symbol_checklist(key, state),
        InputMode::ConfirmDeleteField { .. } => handle_confirm_delete(key, state),
        InputMode::Detail { .. } => handle_detail(key, state),
        InputMode::ExportNodeSelect { .. } => handle_export_node_select(key, state),
        InputMode::ExportNodeFormat { .. } => handle_export_node_format(key, state),
        InputMode::Alert { .. } => handle_alert(key, state),
    }
}

/// Dispatches a key to the active modal.
///
/// The modal reads the key itself and reports a [`TreeAction`], which [`execute`]
/// then performs. The modal is taken out of [`TreeState`] for the duration, so an
/// action that dismisses it simply does not put it back.
fn handle_modal_key(key: event::KeyEvent, state: &mut TreeState) -> io::Result<()> {
    let Some(mut modal) = state.modal.take() else {
        return Ok(());
    };

    match modal.handle_key(key) {
        // The modal consumed the key and stays open.
        None => {
            state.modal = Some(modal);
            Ok(())
        }
        Some(action) => execute(state, action),
    }
}

fn is_leaf_selected(state: &TreeState) -> bool {
    state
        .visible
        .get(state.selected)
        .map(|v| !v.is_parent)
        .unwrap_or(false)
}

fn handle_normal(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_normal(&key) {
        Action::Quit => {
            state.quit = true;
        }
        Action::ReturnToMenu => {
            state.return_to_menu = true;
            state.quit = true;
        }
        Action::NavDown => {
            state.select_next();
        }
        Action::NavUp => {
            state.select_prev();
        }
        Action::NavRight => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            } else {
                state.expand_collapse(true);
            }
        }
        Action::NavLeft => {
            state.collapse_or_select_parent();
        }
        Action::Confirm => {
            if is_leaf_selected(state) {
                state.mode = InputMode::Detail {
                    selected: 0,
                    alert: None,
                };
            } else {
                state.toggle_expand();
            }
        }
        Action::ToggleExpand => {
            state.toggle_expand();
        }
        Action::Cut => {
            state.mode = InputMode::PendingCut;
        }
        Action::PasteStart => {
            if state.cut_buffer.is_some() {
                state.mode = InputMode::PendingPaste;
            }
        }
        Action::AddStart => {
            state.mode = InputMode::PendingAdd;
        }
        Action::OpenDetail => {
            state.mode = InputMode::Detail {
                selected: 0,
                alert: None,
            };
        }
        Action::ToggleSearch => {
            state.mode = InputMode::Search;
        }
        Action::ExportNode => {
            if let Some(node) = state.current_node_ref() {
                state.mode = InputMode::ExportNodeSelect {
                    node: node.clone(),
                    selected: 0,
                };
            }
        }
        Action::Cancel => {
            state.clear_search();
        }
        Action::OpenHelp => {
            state.modal = Some(Modal::Help(Help));
        }
        _ => {}
    }
}

fn handle_pending_cut(key: event::KeyEvent, state: &mut TreeState) {
    if Action::from_pending_cut(&key) == Action::Cut {
        state.cut_selected();
    }
    state.mode = InputMode::Normal;
}

fn handle_pending_paste(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_pending_paste(&key) {
        Action::PasteSibling => {
            state.paste_sibling();
            state.mode = InputMode::Normal;
        }
        Action::PasteBefore => {
            state.paste_sibling_before();
            state.mode = InputMode::Normal;
        }
        Action::PasteChild => {
            state.paste_child();
            state.mode = InputMode::Normal;
        }
        _ => {
            state.mode = InputMode::Normal;
        }
    }
}

fn handle_pending_add(key: event::KeyEvent, state: &mut TreeState) {
    let position = match Action::from_pending_add(&key) {
        Action::AddSiblingNode => AddPosition::SiblingAfter,
        Action::AddBefore => AddPosition::SiblingBefore,
        Action::AddNode => AddPosition::Child,
        _ => {
            state.mode = InputMode::Normal;
            return;
        }
    };
    state.add_position = position;
    state.mode = InputMode::Form(FormState::new_add());
}

fn handle_search(key: event::KeyEvent, state: &mut TreeState) {
    match Action::from_search(&key) {
        Action::ConfirmSearch => {
            state.mode = InputMode::Normal;
        }
        Action::ClearSearch => {
            state.clear_search();
            state.mode = InputMode::Normal;
        }
        Action::Backspace => {
            let mut q = state.search_query.clone();
            q.pop();
            state.set_search(q);
        }
        Action::InsertChar(c) => {
            let mut q = state.search_query.clone();
            q.push(c);
            state.set_search(q);
        }
        _ => {}
    }
}

/// An entry offered by the field-type picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PickerItem {
    Login,
    Password,
    Custom,
    /// Adds a Description extra field (`Custom("Description")`).
    Desc,
}

impl PickerItem {
    fn label(self) -> &'static str {
        match self {
            Self::Login => texts::FIELD_LOGIN,
            Self::Password => texts::FIELD_PASSWORD,
            Self::Custom => texts::FIELD_ITEM_CUSTOM,
            Self::Desc => texts::FIELD_ITEM_DESC,
        }
    }
}

/// Applies an edit to the form's scratch text while at the Input level.
fn scratch_edit<F>(form: &mut FormState, edit: F)
where
    F: FnOnce(&mut String, &mut usize),
{
    if let Some(scratch) = form.scratch.as_mut() {
        edit(&mut scratch.text, &mut scratch.cursor);
    }
}

/// Records a committed change on the form's node.
///
/// On the Add form's first commit this inserts the draft into the tree at
/// [`TreeState::add_position`], switches the form to editing that node, and
/// seeds the undo history. In all cases the resulting node state is pushed onto
/// the history and the tree is marked dirty (triggering autosave).
fn commit_form_change(state: &mut TreeState, form: &mut FormState) {
    if form.is_adding() {
        let placeholder = Node::new(String::new());
        let mut draft = std::mem::replace(&mut form.draft, placeholder);
        if draft.name.is_empty() {
            draft.name = texts::DEFAULT_NAME.to_string();
        }
        match state.insert_node_at_position(state.add_position, draft) {
            Ok(id) => {
                form.node_id = Some(id);
                // The creating commit defines the baseline; it is not itself
                // undoable.
                form.seed_history(&state.root);
            }
            // The position is unavailable (e.g. a sibling slot for the root);
            // keep the draft so the user can keep editing.
            Err(node) => {
                form.draft = node;
                return;
            }
        }
    } else {
        form.push_history(&state.root);
        // A commit can rename the node, but the flat visible list caches names.
        // Rebuild it so the tree shows the new name. (The Add path above already
        // refreshes visibility via `insert_node_at_position`.)
        state.update_visible();
    }
    state.modified = true;
}

/// Records a generation as a committed change.
///
/// Like [`commit_form_change`] it marks the tree modified (triggering autosave);
/// unlike it, consecutive generations coalesce into a single undo step. An Add
/// form's first commit still creates its node and seeds the baseline.
fn commit_generation(state: &mut TreeState, form: &mut FormState) {
    if form.is_adding() {
        commit_form_change(state, form);
        return;
    }
    form.push_history_coalescing(&state.root);
    state.modified = true;
}

/// Writes a generated `value` into the form's focused field.
///
/// Sets the field, keeps the Input-level scratch buffer in step (so a revealed
/// field shows the value immediately and leaving Input does not restore the old
/// value), places the cursor at the end, and records a coalescing commit. Leaves
/// the value untouched and returns `false` when the focused row is not an extra
/// field.
fn write_generated_value(state: &mut TreeState, form: &mut FormState, value: String) -> bool {
    if !form.set_focused_value(&mut state.root, value.clone()) {
        return false;
    }
    // The generator is invoked from the Input level, so the focused text lives
    // in the scratch buffer.
    if form.focus == FormFocus::Input
        && let Some(scratch) = form.scratch.as_mut()
    {
        scratch.text = value;
        scratch.cursor = scratch.text.len();
    }
    form.cursor = form.active_len(&state.root);
    commit_generation(state, form);
    true
}

/// Returns the mode to enter when the Edit/Add form is closed.
///
/// An Edit session (including an Add session whose first commit created its
/// node) returns to the node's Detail overlay; an empty Add session cancels
/// back to Normal.
fn close_form(form: &FormState) -> InputMode {
    if form.is_adding() {
        InputMode::Normal
    } else {
        InputMode::Detail {
            selected: 0,
            alert: None,
        }
    }
}

/// Which generator a field type has, if any.
///
/// An enum rather than a trait: a new variant forces every `match` to be
/// revisited. It is the single source of truth for both the key handler and the
/// form hint. It carries no parameters — those differ per field type (a login
/// word has none, a password has [`PasswordParams`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldGenerator {
    Login,
    Password,
}

impl FieldGenerator {
    /// The generator for `field_type`, or `None` when it has none.
    fn for_field(field_type: Option<&FieldType>) -> Option<Self> {
        match field_type {
            Some(FieldType::Login) => Some(Self::Login),
            Some(FieldType::Password) => Some(Self::Password),
            _ => None,
        }
    }
}

/// Rows of the password-generator modal, in display order.
const GENERATOR_ROW_LENGTH: usize = 0;
const GENERATOR_ROW_SYMBOLS: usize = 1;
const GENERATOR_ROW_GENERATE: usize = 2;
const GENERATOR_ROWS: usize = 3;

/// Columns in the symbol checklist grid.
const SYMBOL_COLUMNS: usize = 3;

fn handle_form(key: event::KeyEvent, state: &mut TreeState) {
    let mut form = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::Form(form) => form,
        other => {
            state.mode = other;
            return;
        }
    };

    form.clamp_selection(&state.root);
    let row = form.current_row(&state.root);

    let action = match form.pending {
        FormPending::Cut => Action::from_form_pending(&key, false),
        FormPending::Paste => Action::from_form_pending(&key, true),
        FormPending::None => match form.focus {
            FormFocus::Nav => Action::from_form_nav(&key),
            FormFocus::Input => Action::from_form_input(&key),
        },
    };

    // Any unmapped key while a field prefix is pending cancels just the prefix.
    if form.pending != FormPending::None && action == Action::Cancel {
        form.pending = FormPending::None;
        state.mode = InputMode::Form(form);
        return;
    }

    match action {
        Action::Generate => {
            // `Ctrl+G` is bound at the Input level and only when the focused
            // row carries a generator (a Login or Password field). Other rows
            // fall through and the form is restored untouched.
            match FieldGenerator::for_field(form.focused_field_type(&state.root)) {
                Some(FieldGenerator::Password) => {
                    let length_input = state.generator.length.value().to_string();
                    let length_cursor = length_input.len();
                    state.mode = InputMode::GeneratePassword(GeneratorModal {
                        params: state.generator,
                        length_input,
                        length_cursor,
                        length_fresh: true,
                        error: None,
                        selected: GENERATOR_ROW_LENGTH,
                        form,
                    });
                    return;
                }
                Some(FieldGenerator::Login) => {
                    // A login word has no parameters, so it is generated on the
                    // spot: no modal, and `Ctrl+G` again rerolls.
                    let value = generator::generate_login();
                    write_generated_value(state, &mut form, value);
                    state.mode = InputMode::Form(form);
                    return;
                }
                None => {}
            }
        }
        Action::Cut if form.pending == FormPending::None => {
            if matches!(row, FormRow::Extra(_)) {
                form.pending = FormPending::Cut;
            }
        }
        Action::Cut => {
            if form.cut_focused_field(&mut state.root) {
                commit_form_change(state, &mut form);
            }
            form.pending = FormPending::None;
        }
        Action::PasteStart => {
            if form.field_cut_buffer.is_some() && matches!(row, FormRow::Extra(_)) {
                form.pending = FormPending::Paste;
            }
        }
        Action::PasteSibling => {
            if form.paste_field_after(&mut state.root) {
                commit_form_change(state, &mut form);
            }
            form.pending = FormPending::None;
        }
        Action::PasteBefore => {
            if form.paste_field_before(&mut state.root) {
                commit_form_change(state, &mut form);
            }
            form.pending = FormPending::None;
        }
        Action::BeginInput if row == FormRow::AddButton => {
            state.mode = InputMode::SelectFieldType { form, selected: 0 };
            return;
        }
        Action::BeginInput => {
            form.begin_input(&state.root);
        }
        Action::ExitInput => {
            if form.commit_input(&mut state.root) {
                commit_form_change(state, &mut form);
            }
        }
        Action::Undo => {
            if form.undo(&mut state.root) {
                state.modified = true;
                // Undo can restore a different name; refresh the visible list.
                state.update_visible();
            }
        }
        Action::Redo => {
            if form.redo(&mut state.root) {
                state.modified = true;
                // Redo can restore a different name; refresh the visible list.
                state.update_visible();
            }
        }
        // Delete on a Name row or the add button has no meaning in Nav.
        Action::DeleteChar if row == FormRow::AddButton => {}
        Action::DeleteChar if row == FormRow::Name && form.focus == FormFocus::Nav => {}
        Action::DeleteChar => {
            if let FormRow::Extra(i) = row
                && form.focus == FormFocus::Nav
            {
                state.mode = InputMode::ConfirmDeleteField {
                    form,
                    target_index: i,
                };
                return;
            }
            scratch_edit(&mut form, |text, cursor| delete_at(text, *cursor));
        }
        Action::Cancel => {
            state.mode = close_form(&form);
            return;
        }
        Action::NextField => {
            let len = form.rows(&state.root).len();
            let next = (form.selected_row + 1) % len;
            form.select_row(&state.root, next);
        }
        Action::PrevField => {
            let len = form.rows(&state.root).len();
            let prev = (form.selected_row + len - 1) % len;
            form.select_row(&state.root, prev);
        }
        Action::CursorLeft => {
            scratch_edit(&mut form, |text, cursor| {
                *cursor = cursor_left(text, *cursor)
            });
        }
        Action::CursorRight => {
            scratch_edit(&mut form, |text, cursor| {
                *cursor = cursor_right(text, *cursor)
            });
        }
        Action::CursorHome => {
            scratch_edit(&mut form, |_, cursor| *cursor = 0);
        }
        Action::CursorEnd => {
            scratch_edit(&mut form, |text, cursor| *cursor = text.len());
        }
        Action::Backspace => {
            scratch_edit(&mut form, |text, cursor| {
                *cursor = backspace_at(text, *cursor);
            });
        }
        Action::InsertChar(c) => {
            scratch_edit(&mut form, |text, cursor| insert_char_at(text, cursor, c));
        }
        _ => {}
    }

    state.mode = InputMode::Form(form);
}

fn handle_select_field_type(key: event::KeyEvent, state: &mut TreeState) {
    let (mut form, selected) = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::SelectFieldType { form, selected } => (form, selected),
        other => {
            state.mode = other;
            return;
        }
    };

    let items = picker_items(&form);
    let selected = selected.min(items.len().saturating_sub(1));

    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.mode = InputMode::Form(form);
        }
        Action::NavDown => {
            let next = (selected + 1) % items.len();
            state.mode = InputMode::SelectFieldType {
                form,
                selected: next,
            };
        }
        Action::NavUp => {
            let prev = (selected + items.len() - 1) % items.len();
            state.mode = InputMode::SelectFieldType {
                form,
                selected: prev,
            };
        }
        Action::SelectIndex(i) => {
            if i < items.len() {
                state.mode = InputMode::SelectFieldType { form, selected: i };
            } else {
                state.mode = InputMode::SelectFieldType { form, selected };
            }
        }
        Action::Confirm => match items[selected] {
            PickerItem::Login => {
                let index = form.push_field(
                    &mut state.root,
                    ExtraField {
                        field_type: FieldType::Login,
                        value: String::new(),
                    },
                );
                form.focus_new_field(&state.root, index);
                commit_form_change(state, &mut form);
                state.mode = InputMode::Form(form);
            }
            PickerItem::Password => {
                let index = form.push_field(
                    &mut state.root,
                    ExtraField {
                        field_type: FieldType::Password,
                        value: String::new(),
                    },
                );
                form.focus_new_field(&state.root, index);
                commit_form_change(state, &mut form);
                state.mode = InputMode::Form(form);
            }
            PickerItem::Custom => {
                state.mode = InputMode::CustomFieldName {
                    form,
                    input: String::new(),
                };
            }
            PickerItem::Desc => {
                let index =
                    form.push_field(&mut state.root, ExtraField::description(String::new()));
                form.focus_new_field(&state.root, index);
                commit_form_change(state, &mut form);
                state.mode = InputMode::Form(form);
            }
        },
        _ => {
            state.mode = InputMode::SelectFieldType { form, selected };
        }
    }
}

/// Items offered by the field-type picker for the given form.
fn picker_items(_form: &FormState) -> Vec<PickerItem> {
    vec![
        PickerItem::Login,
        PickerItem::Password,
        PickerItem::Desc,
        PickerItem::Custom,
    ]
}

fn handle_custom_field_name(key: event::KeyEvent, state: &mut TreeState) {
    let (mut form, input) = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::CustomFieldName { form, input } => (form, input),
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_custom_field_name(&key) {
        Action::Cancel => {
            state.mode = InputMode::SelectFieldType { form, selected: 3 };
        }
        Action::Confirm => {
            let label = if input.is_empty() {
                texts::CUSTOM_FIELD_DEFAULT.to_string()
            } else {
                input
            };
            let index = form.push_field(
                &mut state.root,
                ExtraField {
                    field_type: FieldType::Custom(label),
                    value: String::new(),
                },
            );
            form.focus_new_field(&state.root, index);
            commit_form_change(state, &mut form);
            state.mode = InputMode::Form(form);
        }
        Action::Backspace => {
            let mut new_input = input;
            new_input.pop();
            state.mode = InputMode::CustomFieldName {
                form,
                input: new_input,
            };
        }
        Action::InsertChar(c) => {
            let mut new_input = input;
            new_input.push(c);
            state.mode = InputMode::CustomFieldName {
                form,
                input: new_input,
            };
        }
        _ => {
            state.mode = InputMode::CustomFieldName { form, input };
        }
    }
}

/// Keys for the password-generator parameter modal.
///
/// `Ctrl+G` opened this from the Add/Edit form's Input level. On the length row,
/// the first digit replaces the seeded value and the caret can then be moved with
/// `Left`/`Right`/`Home`/`End`, inserting/removing at most three digits.
/// Generating writes the value into the focused field (and its scratch buffer),
/// commits it (undo entry + autosave) and returns to the form still editing the
/// field. `Esc` returns without touching the field.
fn handle_generate_password(key: event::KeyEvent, state: &mut TreeState) {
    let mut modal = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::GeneratePassword(modal) => modal,
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_generator(&key) {
        Action::Cancel => {
            state.mode = InputMode::Form(modal.form);
        }
        Action::NavDown => {
            modal.selected = (modal.selected + 1) % GENERATOR_ROWS;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::NavUp => {
            modal.selected = (modal.selected + GENERATOR_ROWS - 1) % GENERATOR_ROWS;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::Toggle => {
            if modal.selected == GENERATOR_ROW_SYMBOLS {
                modal.params.symbols.toggle_all();
            }
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::CursorLeft if modal.selected == GENERATOR_ROW_LENGTH => {
            modal.length_fresh = false;
            modal.length_cursor = cursor_left(&modal.length_input, modal.length_cursor);
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::CursorRight if modal.selected == GENERATOR_ROW_LENGTH => {
            modal.length_fresh = false;
            modal.length_cursor = cursor_right(&modal.length_input, modal.length_cursor);
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::CursorHome if modal.selected == GENERATOR_ROW_LENGTH => {
            modal.length_fresh = false;
            modal.length_cursor = 0;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::CursorEnd if modal.selected == GENERATOR_ROW_LENGTH => {
            modal.length_fresh = false;
            modal.length_cursor = modal.length_input.len();
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::InsertChar(c) if modal.selected == GENERATOR_ROW_LENGTH && c.is_ascii_digit() => {
            if modal.length_fresh {
                modal.length_input.clear();
                modal.length_cursor = 0;
                modal.length_fresh = false;
            }
            if modal.length_input.len() < 3 {
                insert_char_at(&mut modal.length_input, &mut modal.length_cursor, c);
            }
            modal.error = None;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::Backspace if modal.selected == GENERATOR_ROW_LENGTH => {
            if modal.length_fresh {
                modal.length_input.clear();
                modal.length_cursor = 0;
                modal.length_fresh = false;
            } else {
                modal.length_cursor = backspace_at(&mut modal.length_input, modal.length_cursor);
            }
            modal.error = None;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::DeleteChar if modal.selected == GENERATOR_ROW_LENGTH => {
            if modal.length_fresh {
                modal.length_input.clear();
                modal.length_cursor = 0;
                modal.length_fresh = false;
            } else {
                delete_at(&mut modal.length_input, modal.length_cursor);
            }
            modal.error = None;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::Confirm if modal.selected == GENERATOR_ROW_LENGTH => {
            modal.selected = GENERATOR_ROW_SYMBOLS;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::Confirm if modal.selected == GENERATOR_ROW_SYMBOLS => {
            modal.selected = 0;
            state.mode = InputMode::SymbolChecklist(modal);
        }
        Action::Confirm if modal.selected == GENERATOR_ROW_GENERATE => {
            match modal.length_input.parse::<PasswordLength>() {
                Ok(length) => {
                    let params = PasswordParams {
                        length,
                        symbols: modal.params.symbols,
                    };
                    let Some(FieldGenerator::Password) =
                        FieldGenerator::for_field(modal.form.focused_field_type(&state.root))
                    else {
                        // The row lost its generator (defensive); leave the
                        // value untouched and return to the form.
                        state.mode = InputMode::Form(modal.form);
                        return;
                    };
                    let value = generator::generate_password(&params);
                    write_generated_value(state, &mut modal.form, value);
                    state.generator = params;
                    state.mode = InputMode::Form(modal.form);
                }
                Err(error) => {
                    modal.error = Some(error);
                    state.mode = InputMode::GeneratePassword(modal);
                }
            }
        }
        _ => {
            state.mode = InputMode::GeneratePassword(modal);
        }
    }
}

/// Keys for the per-symbol checklist opened from the generator modal.
///
/// The 17 symbols are laid out in a [`SYMBOL_COLUMNS`]-wide grid; `h`/`l`
/// move one cell and `j`/`k` move one row. `Space`/`Enter` toggle the focused
/// symbol, `Esc` returns to the parameter modal focused on the summary row.
fn handle_symbol_checklist(key: event::KeyEvent, state: &mut TreeState) {
    let mut modal = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::SymbolChecklist(modal) => modal,
        other => {
            state.mode = other;
            return;
        }
    };

    let last = generator::SYMBOLS.len() - 1;
    match Action::from_symbol_checklist(&key) {
        Action::Cancel => {
            modal.selected = GENERATOR_ROW_SYMBOLS;
            state.mode = InputMode::GeneratePassword(modal);
        }
        Action::NavLeft => {
            modal.selected = modal.selected.saturating_sub(1);
            state.mode = InputMode::SymbolChecklist(modal);
        }
        Action::NavRight => {
            modal.selected = (modal.selected + 1).min(last);
            state.mode = InputMode::SymbolChecklist(modal);
        }
        Action::NavUp => {
            modal.selected = modal.selected.saturating_sub(SYMBOL_COLUMNS);
            state.mode = InputMode::SymbolChecklist(modal);
        }
        Action::NavDown => {
            modal.selected = (modal.selected + SYMBOL_COLUMNS).min(last);
            state.mode = InputMode::SymbolChecklist(modal);
        }
        Action::Toggle => {
            if let Some(&symbol) = generator::SYMBOLS.get(modal.selected) {
                modal.params.symbols.toggle(symbol);
            }
            state.mode = InputMode::SymbolChecklist(modal);
        }
        _ => {
            state.mode = InputMode::SymbolChecklist(modal);
        }
    }
}

fn handle_confirm_delete(key: event::KeyEvent, state: &mut TreeState) {
    let (mut form, target_index) = match std::mem::replace(&mut state.mode, InputMode::Normal) {
        InputMode::ConfirmDeleteField { form, target_index } => (form, target_index),
        other => {
            state.mode = other;
            return;
        }
    };

    if Action::from_confirm(&key) == Action::Yes && form.remove_field(&mut state.root, target_index)
    {
        form.focus = FormFocus::Nav;
        form.clamp_selection(&state.root);
        form.cursor = form.active_len(&state.root);
        commit_form_change(state, &mut form);
    }
    state.mode = InputMode::Form(form);
}

fn copy_to_clipboard(text: &str) {
    let cmds: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["pbcopy"]]
    } else if cfg!(target_os = "linux") {
        &[
            &["wl-copy"],
            &["xclip", "-selection", "clipboard"],
            &["xsel", "-b"],
        ]
    } else {
        return;
    };
    for args in cmds {
        if let Ok(mut child) = std::process::Command::new(args[0])
            .args(&args[1..])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
            break;
        }
    }
}

fn handle_detail(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (selected, alert) = match mode {
        InputMode::Detail { selected, alert } => (selected, alert),
        other => {
            state.mode = other;
            return;
        }
    };

    if alert.is_some() {
        state.mode = InputMode::Detail {
            selected,
            alert: None,
        };
        return;
    }

    let field_count = state.current_node_ref().map_or(0, |n| n.extra_fields.len());

    match Action::from_detail(&key) {
        Action::Cancel => {
            state.mode = InputMode::Normal;
        }
        Action::NavDown => {
            let last = field_count.saturating_sub(1);
            let new_sel = (selected + 1).min(last);
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::NavUp => {
            let new_sel = selected.saturating_sub(1);
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::NextField if field_count > 0 => {
            let new_sel = (selected + 1) % field_count;
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::PrevField if field_count > 0 => {
            let new_sel = (selected + field_count - 1) % field_count;
            state.mode = InputMode::Detail {
                selected: new_sel,
                alert: None,
            };
        }
        Action::CopyField => {
            let text = state
                .current_node_ref()
                .and_then(|node| node.extra_fields.get(selected).map(|ef| ef.value.clone()));
            if let Some(ref t) = text
                && !t.is_empty()
            {
                copy_to_clipboard(t);
            }
            state.mode = InputMode::Detail {
                selected,
                alert: Some(texts::COPY_CHECK.to_string()),
            };
        }
        Action::EditNode => {
            if let Some(node) = state.current_node_ref() {
                let form = FormState::new_edit(node);
                state.mode = InputMode::Form(form);
            } else {
                state.mode = InputMode::Detail {
                    selected,
                    alert: None,
                };
            }
        }
        _ => {
            state.mode = InputMode::Detail {
                selected,
                alert: None,
            };
        }
    }
}

fn handle_export_node_select(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node, selected) = match mode {
        InputMode::ExportNodeSelect { node, selected } => (node, selected),
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.mode = InputMode::Normal;
        }
        Action::NavDown => {
            state.mode = InputMode::ExportNodeSelect {
                node,
                selected: (selected + 1) % 2,
            };
        }
        Action::NavUp => {
            state.mode = InputMode::ExportNodeSelect {
                node,
                selected: (selected + 1) % 2,
            };
        }
        Action::Confirm => {
            let include_children = selected == 1;
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: 0,
            };
        }
        Action::SelectIndex(i) if i < 2 => {
            state.mode = InputMode::ExportNodeSelect { node, selected: i };
        }
        _ => {
            state.mode = InputMode::ExportNodeSelect { node, selected };
        }
    }
}

fn handle_export_node_format(key: event::KeyEvent, state: &mut TreeState) {
    let mode = std::mem::replace(&mut state.mode, InputMode::Normal);
    let (node, include_children, selected_format) = match mode {
        InputMode::ExportNodeFormat {
            node,
            include_children,
            selected_format,
        } => (node, include_children, selected_format),
        other => {
            state.mode = other;
            return;
        }
    };

    match Action::from_select_field_type(&key) {
        Action::Cancel => {
            state.mode = InputMode::Normal;
        }
        Action::NavDown => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: (selected_format + 1) % 3,
            };
        }
        Action::NavUp => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: (selected_format + 2) % 3,
            };
        }
        Action::SelectIndex(i) if i < 3 => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format: i,
            };
        }
        Action::Confirm => {
            use crate::file_handling::export::{ExportFormat, export_tree};

            let format = match selected_format {
                0 => ExportFormat::Json,
                1 => ExportFormat::Xml,
                _ => ExportFormat::Csv,
            };

            let node_name = node.name.clone();
            let export_node = if include_children {
                node
            } else {
                let mut n = node;
                n.children = Vec::new();
                n
            };

            let base_path = match &state.active_vault_path {
                Some(p) => p.clone(),
                None => {
                    state.mode = InputMode::Alert {
                        message: texts::MSG_NO_ACTIVE_FILE.to_string(),
                    };
                    return;
                }
            };

            let path = std::path::Path::new(&base_path);
            let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("export");
            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("age");
            let node_name_clean: String = node_name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            let export_path = parent.join(format!("{}_{}.{}", stem, node_name_clean, ext));
            let export_path_str = export_path.to_string_lossy().to_string();

            match export_tree(&export_node, &export_path_str, format) {
                Ok(out_path) => {
                    state.mode = InputMode::Alert {
                        message: format!("{}{}", texts::MSG_EXPORT_NODE_SUCCESS, out_path),
                    };
                }
                Err(e) => {
                    state.mode = InputMode::Alert {
                        message: format!("{e}"),
                    };
                }
            }
        }
        _ => {
            state.mode = InputMode::ExportNodeFormat {
                node,
                include_children,
                selected_format,
            };
        }
    }
}

fn handle_alert(key: event::KeyEvent, state: &mut TreeState) {
    let _ = key;
    state.mode = InputMode::Normal;
}

fn ui(frame: &mut Frame, state: &mut TreeState) {
    let area = frame.area();

    // Own the background: paint SURFACE across the whole screen first so the
    // terminal's own background never shows through.
    paint_background(frame, area);

    let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

    TreeWidget::render(frame, layout[0], state);

    let (mode_str, mode_style, hint) = status_bar_parts(state);
    render_status_bar(frame, layout[1], mode_str, mode_style.style(), hint);

    match &state.mode {
        InputMode::Search => render_search_overlay(frame, area, state),
        InputMode::Form(form) => {
            let title = if form.is_adding() {
                texts::OVERLAY_ADD_NODE
            } else {
                texts::OVERLAY_EDIT_NODE
            };
            render_edit_overlay(frame, area, form, &state.root, title);
        }
        InputMode::SelectFieldType { .. } => render_select_field_type_overlay(frame, area, state),
        InputMode::CustomFieldName { .. } => render_custom_field_name_overlay(frame, area, state),
        InputMode::GeneratePassword(modal) => render_generate_password_overlay(frame, area, modal),
        InputMode::SymbolChecklist(modal) => render_symbol_checklist_overlay(frame, area, modal),
        InputMode::ConfirmDeleteField { .. } => render_confirm_delete_overlay(frame, area, state),
        InputMode::Detail { .. } => {
            render_detail_overlay(frame, area, state);
            if matches!(state.mode, InputMode::Detail { ref alert, .. } if alert.is_some()) {
                render_copy_alert_overlay(frame, area);
            }
        }
        InputMode::ExportNodeSelect { .. } => {
            render_export_node_select_overlay(frame, area, state);
        }
        InputMode::ExportNodeFormat { .. } => {
            render_export_node_format_overlay(frame, area, state);
        }
        InputMode::Alert { message } => {
            render_alert_overlay(frame, area, message);
        }
        _ => {}
    }

    if let Some(Modal::Help(help)) = &state.modal {
        help.render(frame, area);
    }

    match state.save_exit {
        SaveExit::ConfirmDiscard => render_discard_confirm_overlay(frame, area),
        _ => {
            if let Some(message) = state.save_state.overlay_message() {
                render_save_error_overlay(frame, area, &message);
            }
        }
    }
}

/// Maps the active tree mode (or a parked save overlay) to the unified
/// status-bar parts: `(mode label, badge style, context hint)`.
///
/// A parked save overlay takes priority over the mode because it is drawn on
/// top of it. It carries no badge: it is a transient prompt, not a mode.
fn status_bar_parts(state: &TreeState) -> (&'static str, ElementStyle, &'static str) {
    match state.save_exit {
        SaveExit::ConfirmDiscard => {
            return ("", ElementStyle::ModeNormal, texts::HINT_DISCARD_CONFIRM);
        }
        SaveExit::Blocked => {
            return ("", ElementStyle::ModeNormal, texts::HINT_SAVE_EXIT_BLOCKED);
        }
        SaveExit::None => {}
    }
    if state.save_state.overlay_message().is_some() {
        return ("", ElementStyle::ModeNormal, texts::HINT_DETAIL_ALERT);
    }

    if let Some(Modal::Help(help)) = &state.modal {
        return help.status_parts();
    }

    match &state.mode {
        InputMode::Normal => (
            texts::MODE_NAV,
            ElementStyle::ModeNormal,
            if state.cut_buffer.is_some() {
                texts::HINT_TREE_PASTE
            } else {
                texts::HINT_TREE
            },
        ),
        InputMode::Search => (
            texts::MODE_SEARCH,
            ElementStyle::ModeSearch,
            texts::HINT_SEARCH,
        ),
        InputMode::PendingCut => (
            texts::MODE_PENDING_CUT,
            ElementStyle::ModePendingCut,
            texts::HINT_PENDING_CUT,
        ),
        InputMode::PendingPaste => (
            texts::MODE_PASTE,
            ElementStyle::ModePendingPaste,
            texts::HINT_PASTE,
        ),
        InputMode::PendingAdd => (
            texts::MODE_PENDING_ADD,
            ElementStyle::ModePendingAdd,
            texts::HINT_PENDING_ADD,
        ),
        InputMode::Detail { alert, .. } => (
            texts::MODE_DETAIL,
            ElementStyle::ModeDetail,
            if alert.is_some() {
                texts::HINT_DETAIL_ALERT
            } else {
                texts::HINT_DETAIL
            },
        ),
        InputMode::Form(form) => {
            let (mode_str, mode_style) = match form.pending {
                FormPending::Cut => (texts::MODE_PENDING_CUT, ElementStyle::ModePendingCut),
                FormPending::Paste => (texts::MODE_PASTE, ElementStyle::ModePendingPaste),
                FormPending::None if form.is_adding() => (texts::MODE_ADD, ElementStyle::ModeAdd),
                FormPending::None => (texts::MODE_EDIT, ElementStyle::ModeEdit),
            };
            (mode_str, mode_style, form_hint(form, &state.root))
        }
        InputMode::SelectFieldType { .. } => (
            texts::MODE_FIELD_TYPE,
            ElementStyle::ModeFieldType,
            texts::HINT_SELECT_FIELD_TYPE,
        ),
        InputMode::CustomFieldName { .. } => (
            texts::MODE_CUSTOM_NAME,
            ElementStyle::ModeCustomName,
            texts::HINT_CUSTOM_FIELD_NAME,
        ),
        InputMode::GeneratePassword(_) => (
            texts::MODE_GENERATOR,
            ElementStyle::ModeGenerator,
            texts::HINT_GENERATOR,
        ),
        InputMode::SymbolChecklist(_) => (
            texts::MODE_SYMBOLS,
            ElementStyle::ModeSymbols,
            texts::HINT_SYMBOLS,
        ),
        InputMode::ConfirmDeleteField { .. } => (
            texts::MODE_CONFIRM,
            ElementStyle::ModeConfirmDelete,
            texts::HINT_CONFIRM_DELETE,
        ),
        InputMode::ExportNodeSelect { .. } => (
            texts::MODE_EXPORT_SELECT,
            ElementStyle::ModeFieldType,
            texts::HINT_EXPORT_NODE_SELECT,
        ),
        InputMode::ExportNodeFormat { .. } => (
            texts::MODE_EXPORT_FORMAT,
            ElementStyle::ModeFieldType,
            texts::HINT_EXPORT_NODE_FORMAT,
        ),
        InputMode::Alert { .. } => (
            texts::MODE_NORMAL,
            ElementStyle::ModeNormal,
            texts::HINT_DETAIL_ALERT,
        ),
    }
}

fn render_search_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 1,
        width: area.width / 2,
        height: 3,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SEARCH)
        .borders(Borders::ALL)
        .style(ElementStyle::SearchBlock.style());

    let text = Text::from(Line::from(vec![Span::styled(
        format!("/{}", state.search_query),
        ElementStyle::SearchText.style(),
    )]));

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(text), inner);
}

fn render_export_node_select_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let selected = match &state.mode {
        InputMode::ExportNodeSelect { selected, .. } => *selected,
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 3,
        width: area.width / 2,
        height: 6,
    };

    let block = Block::default()
        .title(texts::OVERLAY_EXPORT_NODE)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in texts::EXPORT_NODE_OPTIONS.iter().enumerate() {
        lines.push(Line::from(selected_span(item, i == selected)));
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_export_node_format_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let selected_format = match &state.mode {
        InputMode::ExportNodeFormat {
            selected_format, ..
        } => *selected_format,
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 3,
        width: area.width / 2,
        height: 7,
    };

    let block = Block::default()
        .title(texts::EXPORT_SELECT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in texts::EXPORT_TYPE_ITEMS.iter().enumerate() {
        lines.push(Line::from(selected_span(item, i == selected_format)));
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_alert_overlay(frame: &mut Frame, area: Rect, message: &str) {
    let overlay_area = Rect {
        x: area.width / 6,
        y: area.height / 2 - 2,
        width: area.width * 2 / 3,
        height: 4,
    };

    let block = Block::default()
        .title(texts::ALERT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::CopiedBlock.style());

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", message),
            ElementStyle::CopiedText.style(),
        )),
        Line::from(""),
    ];

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(&block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Renders the save-error overlay.
///
/// The retry / quit-without-saving hint and the dismiss hint live in the
/// status bar (see [`status_bar_parts`]); the overlay carries only the message.
fn render_save_error_overlay(frame: &mut Frame, area: Rect, message: &str) {
    let width = (area.width.saturating_mul(2) / 3).max(1);
    let inner_width = width.saturating_sub(2).max(1) as usize;
    // Approximate display width: non-ASCII characters are counted as two
    // cells, covering CJK and most emoji. Include the leading space.
    let display_width = message
        .chars()
        .map(|c| if c.is_ascii() { 1 } else { 2 })
        .sum::<usize>()
        .saturating_add(1);
    let message_rows = display_width.div_ceil(inner_width).max(1) as u16;
    let height = message_rows.saturating_add(3).min(area.height).max(1);

    let overlay_area = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(height) / 2,
        width,
        height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SAVE_ERROR)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", message),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(&block, overlay_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

/// Renders the "quit without saving?" confirmation overlay.
///
/// The confirm/dismiss hint lives in the status bar (see [`status_bar_parts`]).
fn render_discard_confirm_overlay(frame: &mut Frame, area: Rect) {
    let overlay_area = Rect {
        x: area.width / 6,
        y: area.height / 2 - 2,
        width: area.width * 2 / 3,
        height: 4,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SAVE_ERROR)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let lines = vec![
        Line::from(Span::styled(
            format!(" {}", texts::MSG_DISCARD_CONFIRM),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

    let inner = block.inner(overlay_area);
    frame.render_widget(Clear, overlay_area);
    frame.render_widget(&block, overlay_area);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

fn make_overlay_area(area: Rect, extra_fields_len: usize, base_content: u16) -> Rect {
    let n = extra_fields_len as u16;
    let content_height = base_content.saturating_add(n);
    let overlay_height = (content_height + 2).min(area.height.saturating_sub(2));
    Rect {
        x: area.width / 5,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width * 3 / 5,
        height: overlay_height,
    }
}

/// Renders one form row as a list-style line.
///
/// - Nav: the focused row gets `▸ ` + `FormRowFocused` (like Detail); other
///   rows get `  ` + dimmed label/text.
/// - Input: the focused row shows the inverted cursor; other rows are dimmed.
fn form_row_line(
    label: &str,
    text: &str,
    cursor: usize,
    is_focused: bool,
    is_editing: bool,
) -> Line<'static> {
    let prefix = if is_focused {
        Icon::SelectionArrow.as_str()
    } else {
        Icon::SelectionPad.as_str()
    };

    if is_focused && is_editing {
        let mut spans = vec![Span::raw(prefix.to_string())];
        spans.extend(cursor_field_spans(label, text, cursor, true));
        return Line::from(spans);
    }

    let style = if is_focused {
        ElementStyle::FormRowFocused.style()
    } else {
        ElementStyle::TreeRow.style()
    };
    Line::from(vec![Span::styled(format!("{prefix}{label}{text}"), style)])
}

/// Renders one extra-field row, masking password values unless the row is
/// actively being edited (`Input` level).
fn render_extra_field_line(
    ef: &ExtraField,
    text: &str,
    cursor: usize,
    is_focused: bool,
    is_editing: bool,
) -> Line<'static> {
    let revealed = is_focused && is_editing;
    let display_val = if revealed {
        Cow::Borrowed(text)
    } else {
        ef.field_type.display_value(text)
    };
    let label = format!("{}: ", ef.field_type.label());
    form_row_line(&label, &display_val, cursor, is_focused, is_editing)
}

/// Resolves the text and cursor to render for a field row.
///
/// The focused row at the Input level shows the scratch buffer; every other
/// row shows the node's stored value.
fn field_display<'a>(form: &'a FormState, value: &'a str, focused: bool) -> (Cow<'a, str>, usize) {
    match form
        .scratch
        .as_ref()
        .filter(|_| focused && form.focus == FormFocus::Input)
    {
        Some(scratch) => (Cow::Borrowed(scratch.text.as_str()), scratch.cursor),
        None => (Cow::Borrowed(value), form.cursor),
    }
}

fn render_edit_overlay(frame: &mut Frame, area: Rect, form: &FormState, root: &Node, title: &str) {
    let node = form.node(root);
    let rows = form.rows(root);
    let current = form.selected_row.min(rows.len() - 1);
    let is_editing = form.focus == FormFocus::Input;

    let overlay_area = make_overlay_area(area, node.extra_fields.len(), 4);

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .style(ElementStyle::EditBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        let focused = i == current;
        match row {
            FormRow::Name => {
                let (text, cursor) = field_display(form, &node.name, focused);
                lines.push(form_row_line(
                    texts::LABEL_NAME,
                    &text,
                    cursor,
                    focused,
                    is_editing,
                ));
            }
            FormRow::Extra(idx) => {
                if let Some(ef) = node.extra_fields.get(*idx) {
                    let (text, cursor) = field_display(form, &ef.value, focused);
                    lines.push(render_extra_field_line(
                        ef, &text, cursor, focused, is_editing,
                    ));
                }
            }
            FormRow::AddButton => {
                let style = if focused {
                    ElementStyle::FormRowFocused.style()
                } else {
                    ElementStyle::TreeRow.style()
                };
                let prefix = if focused {
                    Icon::SelectionArrow.as_str()
                } else {
                    Icon::SelectionPad.as_str()
                };
                lines.push(Line::from(Span::styled(
                    format!("{prefix}{}", texts::ADD_FIELD_BUTTON),
                    style,
                )));
            }
        }
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Selects the help-bar hint for the Add/Edit form.
fn form_hint(form: &FormState, root: &Node) -> &'static str {
    match form.pending {
        FormPending::Cut => return texts::HINT_FIELD_CUT,
        FormPending::Paste => return texts::HINT_FIELD_PASTE,
        FormPending::None => {}
    }
    if form.focus == FormFocus::Input {
        return if FieldGenerator::for_field(form.focused_field_type(root)).is_some() {
            texts::HINT_FORM_GENERATOR_INPUT
        } else {
            texts::HINT_FORM_INPUT
        };
    }
    match form.current_row(root) {
        FormRow::AddButton => texts::HINT_FORM_ADD,
        FormRow::Extra(_) => {
            if form.field_cut_buffer.is_some() {
                texts::HINT_FORM_EXTRA_PASTE
            } else {
                texts::HINT_FORM_EXTRA
            }
        }
        FormRow::Name => texts::HINT_FORM,
    }
}

fn render_select_field_type_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let (form, selected_type) = match &state.mode {
        InputMode::SelectFieldType { form, selected } => (form, *selected),
        _ => return,
    };
    let items = picker_items(form);

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 4,
        width: area.width / 2,
        height: 8,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SELECT_FIELD_TYPE)
        .borders(Borders::ALL)
        .style(ElementStyle::FieldTypeBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    for (i, item) in items.iter().enumerate() {
        let label = format!("{}. {}", i + 1, item.label());
        lines.push(Line::from(selected_span(&label, i == selected_type)));
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_custom_field_name_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let input = match &state.mode {
        InputMode::CustomFieldName { input, .. } => input.clone(),
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 2,
        width: area.width / 2,
        height: 4,
    };

    let block = Block::default()
        .title(texts::OVERLAY_CUSTOM_FIELD_NAME)
        .borders(Borders::ALL)
        .style(ElementStyle::CustomNameBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(cursor_field_spans(
            texts::LABEL_NAME,
            &input,
            input.len(),
            true,
        )),
        Line::from(""),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Renders the password-generator parameter modal.
fn render_generate_password_overlay(frame: &mut Frame, area: Rect, modal: &GeneratorModal) {
    let has_error = modal.error.is_some();
    let content_height = 3 + u16::from(has_error);
    let overlay_height = (content_height + 2).min(area.height);
    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height.saturating_sub(overlay_height) / 2,
        width: area.width / 2,
        height: overlay_height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_GENERATE_PASSWORD)
        .borders(Borders::ALL)
        .style(ElementStyle::GeneratorBlock.style());
    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    if modal.selected == GENERATOR_ROW_LENGTH {
        let mut spans = vec![Span::raw(Icon::SelectionArrow.as_str().to_string())];
        spans.extend(cursor_field_spans(
            texts::LABEL_GENERATOR_LENGTH,
            &modal.length_input,
            modal.length_cursor,
            true,
        ));
        lines.push(Line::from(spans));
    } else {
        let length_label = format!("{}{}", texts::LABEL_GENERATOR_LENGTH, modal.length_input);
        lines.push(Line::from(selected_span(&length_label, false)));
    }

    let selected = modal.params.symbols.len();
    let total = generator::SYMBOLS.len();
    let symbols_label = format!("{}: {selected}/{total}", texts::LABEL_GENERATOR_SYMBOLS);
    lines.push(Line::from(selected_span(
        &symbols_label,
        modal.selected == GENERATOR_ROW_SYMBOLS,
    )));

    lines.push(Line::from(selected_span(
        texts::LABEL_GENERATOR_RUN,
        modal.selected == GENERATOR_ROW_GENERATE,
    )));

    if modal.error.is_some() {
        lines.push(Line::from(Span::styled(
            texts::ERROR_GENERATOR_LENGTH,
            ElementStyle::GeneratorError.style(),
        )));
    }

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Renders the per-symbol checklist as a grid of checkbox cells.
fn render_symbol_checklist_overlay(frame: &mut Frame, area: Rect, modal: &GeneratorModal) {
    let rows = u16::try_from(generator::SYMBOLS.len().div_ceil(SYMBOL_COLUMNS))
        .expect("symbol checklist rows fit in a terminal height");
    let content_height = rows;
    let overlay_height = (content_height + 2).min(area.height);
    let width = u16::try_from(SYMBOL_COLUMNS * 9)
        .expect("symbol checklist width fits in a terminal")
        .min(area.width);
    let overlay_area = Rect {
        x: area.width.saturating_sub(width) / 2,
        y: area.height.saturating_sub(overlay_height) / 2,
        width,
        height: overlay_height,
    };

    let block = Block::default()
        .title(texts::OVERLAY_SYMBOLS)
        .borders(Borders::ALL)
        .style(ElementStyle::SymbolsBlock.style());
    let inner = block.inner(overlay_area);

    let mut lines: Vec<Line> = Vec::new();
    for row in 0..rows {
        let mut spans: Vec<Span> = Vec::new();
        for column in 0..SYMBOL_COLUMNS {
            let index = usize::from(row) * SYMBOL_COLUMNS + column;
            let Some(&symbol) = generator::SYMBOLS.get(index) else {
                break;
            };
            let marker = if modal.params.symbols.contains(symbol) {
                Icon::CheckboxOn
            } else {
                Icon::CheckboxOff
            };
            let style = if index == modal.selected {
                ElementStyle::ListRowSelected.style()
            } else {
                ElementStyle::CursorNormalText.style()
            };
            let cell = format!("{} {}", marker.as_str(), symbol);
            spans.push(Span::styled(format!("{cell:<9}"), style));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_confirm_delete_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let label = match &state.mode {
        InputMode::ConfirmDeleteField { form, target_index } => form
            .node(&state.root)
            .extra_fields
            .get(*target_index)
            .map(|ef| ef.field_type.label())
            .unwrap_or_default(),
        _ => return,
    };

    let overlay_area = Rect {
        x: area.width / 4,
        y: area.height / 2 - 2,
        width: area.width / 2,
        height: 4,
    };

    let block = Block::default()
        .title(texts::OVERLAY_CONFIRM_DELETE)
        .borders(Borders::ALL)
        .style(ElementStyle::ConfirmDeleteBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(Span::styled(
            format!(
                "{}{}{}",
                texts::PROMPT_DELETE_FIELD_START,
                label,
                texts::PROMPT_DELETE_FIELD_END
            ),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_copy_alert_overlay(frame: &mut Frame, area: Rect) {
    let overlay_area = Rect {
        x: area.width / 3,
        y: area.height / 2 - 2,
        width: area.width / 3,
        height: 4,
    };

    let block = Block::default()
        .title(texts::OVERLAY_COPIED)
        .borders(Borders::ALL)
        .style(ElementStyle::CopiedBlock.style());

    let inner = block.inner(overlay_area);
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            texts::COPY_CHECK,
            ElementStyle::CopiedText.style(),
        )),
    ];

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

fn render_detail_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let node = match state.current_node_ref() {
        Some(n) => n,
        None => return,
    };

    let selected = match &state.mode {
        InputMode::Detail { selected, .. } => *selected,
        _ => 0,
    };

    let overlay_area = make_overlay_area(area, node.extra_fields.len(), 3);

    let block = Block::default()
        .title(format!(" {} ", node.name))
        .borders(Borders::ALL)
        .style(ElementStyle::DetailBlock.style());

    let inner = block.inner(overlay_area);
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(""));

    for (i, ef) in node.extra_fields.iter().enumerate() {
        let display_val = ef.field_type.display_value(&ef.value);
        let is_sel = selected == i;
        let text = format!("{}: {}", ef.field_type.label(), display_val);
        lines.push(Line::from(selected_span(&text, is_sel)));
    }

    lines.push(Line::from(""));

    frame.render_widget(Clear, overlay_area);
    frame.render_widget(block, overlay_area);
    frame.render_widget(Paragraph::new(Text::from(lines)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    /// A synthetic file error for tests that need a `SaveError::Failed`.
    fn failed(message: &str) -> SaveError {
        SaveError::Failed(crate::file_handling::FileHandlingError::Io(
            io::Error::other(message),
        ))
    }

    fn editing_state() -> TreeState {
        let root = Node::new("Root");
        let mut state = TreeState::new(root);
        let node = state.root.clone();
        state.mode = InputMode::Form(FormState::new_edit(&node));
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
        state.mode = InputMode::Form(FormState::new_edit(&node));
        state
    }

    fn form_of(state: &TreeState) -> &FormState {
        match &state.mode {
            InputMode::Form(form) => form,
            other => panic!("expected Form mode, got {other:?}"),
        }
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
        let InputMode::Form(form) = &mut state.mode else {
            panic!("expected Form mode");
        };
        form.select_row(&state.root, index);
    }

    fn type_text(state: &mut TreeState, text: &str) {
        for c in text.chars() {
            handle_form(key(KeyCode::Char(c)), state);
        }
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn editing_state_with_password(value: &str) -> TreeState {
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Password,
            value: value.to_string(),
        });
        let mut state = TreeState::new(node);
        let node = state.root.clone();
        state.mode = InputMode::Form(FormState::new_edit(&node));
        state
    }

    fn password_value(state: &TreeState) -> &str {
        &form_node(state).extra_fields[0].value
    }

    fn generator_modal(state: &TreeState) -> &GeneratorModal {
        match &state.mode {
            InputMode::GeneratePassword(modal) | InputMode::SymbolChecklist(modal) => modal,
            other => panic!("expected generator mode, got {other:?}"),
        }
    }

    fn open_generator(state: &mut TreeState) {
        select_row(state, 1);
        if form_of(state).focus == FormFocus::Nav {
            handle_form(key(KeyCode::Char('i')), state);
        }
        handle_form(ctrl(KeyCode::Char('g')), state);
        assert!(matches!(state.mode, InputMode::GeneratePassword(_)));
    }

    fn move_generator_to(state: &mut TreeState, target: usize) {
        while generator_modal(state).selected < target {
            dispatch_key(key(KeyCode::Char('j')), state);
        }
        while generator_modal(state).selected > target {
            dispatch_key(key(KeyCode::Char('k')), state);
        }
    }

    fn set_generator_length(state: &mut TreeState, digits: &str) {
        while !generator_modal(state).length_input.is_empty() {
            dispatch_key(key(KeyCode::Backspace), state);
        }
        for digit in digits.chars() {
            dispatch_key(key(KeyCode::Char(digit)), state);
        }
    }

    fn press_generate(state: &mut TreeState) {
        move_generator_to(state, GENERATOR_ROW_GENERATE);
        dispatch_key(key(KeyCode::Enter), state);
    }

    #[test]
    fn form_opens_in_nav_and_i_enters_input() {
        let mut state = editing_state();
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        handle_form(key(KeyCode::Char('i')), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Input);
    }

    #[test]
    fn form_nav_enter_also_enters_input() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Input);
    }

    #[test]
    fn form_nav_j_moves_row_to_extra_or_add_button() {
        let mut state = editing_state_with_fields();
        handle_form(key(KeyCode::Char('j')), &mut state);
        let form = form_of(&state);
        assert_eq!(form.selected_row, 1); // moved to Extra(0)
        assert_eq!(form.current_row(&state.root), FormRow::Extra(0));
    }

    #[test]
    fn form_nav_esc_closes_to_detail() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Esc), &mut state);
        assert!(matches!(state.mode, InputMode::Detail { .. }));
    }

    #[test]
    fn empty_add_form_esc_cancels_to_normal() {
        let mut state = TreeState::new(Node::new("Root"));
        state.mode = InputMode::Form(FormState::new_add());
        handle_form(key(KeyCode::Esc), &mut state);
        assert!(matches!(state.mode, InputMode::Normal));
    }

    #[test]
    fn typing_is_committed_on_input_exit() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "!");
        // The node is untouched while typing.
        assert_eq!(form_node(&state).name, "Root");
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_node(&state).name, "Root!");
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        assert!(state.modified);
    }

    #[test]
    fn renaming_a_leaf_updates_the_visible_tree() {
        let leaf = Node::new("Old");
        let leaf_id = leaf.id;
        let root = Node::with_children("Root", vec![leaf]);
        let mut state = TreeState::new(root);
        state.expanded.insert(state.root.id);
        state.update_visible();
        assert_eq!(state.visible.len(), 2);

        state.select_by_id(leaf_id);
        let node = state.current_node_ref().expect("leaf exists").clone();
        state.mode = InputMode::Form(FormState::new_edit(&node));

        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "!");
        handle_form(key(KeyCode::Enter), &mut state);

        let visible_leaf = state
            .visible
            .iter()
            .find(|v| v.id == leaf_id)
            .expect("leaf stays visible");
        assert_eq!(visible_leaf.name, "Old!");

        // Undo and redo must also refresh the cached name.
        handle_form(key(KeyCode::Char('u')), &mut state);
        let visible_leaf = state.visible.iter().find(|v| v.id == leaf_id).unwrap();
        assert_eq!(visible_leaf.name, "Old");

        handle_form(key(KeyCode::Char('r')), &mut state);
        let visible_leaf = state.visible.iter().find(|v| v.id == leaf_id).unwrap();
        assert_eq!(visible_leaf.name, "Old!");
    }

    #[test]
    fn leaving_input_unchanged_does_not_commit() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        handle_form(key(KeyCode::Esc), &mut state);
        assert_eq!(form_node(&state).name, "Root");
        assert!(!state.modified);
        assert_eq!(form_of(&state).history.len(), 1);
    }

    #[test]
    fn undo_and_redo_restore_previous_state() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "!");
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_node(&state).name, "Root!");

        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(form_node(&state).name, "Root");
        handle_form(key(KeyCode::Char('r')), &mut state);
        assert_eq!(form_node(&state).name, "Root!");
    }

    #[test]
    fn undo_without_history_is_noop() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(form_node(&state).name, "Root");
        assert!(matches!(state.mode, InputMode::Form(_)));
    }

    #[test]
    fn new_edit_after_undo_truncates_redo_tail() {
        let mut state = editing_state();
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "1");
        handle_form(key(KeyCode::Enter), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert!(form_of(&state).can_redo());

        // A new commit discards the redo tail.
        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "2");
        handle_form(key(KeyCode::Enter), &mut state);
        assert_eq!(form_node(&state).name, "Root2");
        assert!(!form_of(&state).can_redo());
    }

    #[test]
    fn add_form_starts_with_only_name_and_add_button() {
        let state = TreeState::new(Node::new("Root"));
        let form = FormState::new_add();
        assert_eq!(
            form.rows(&state.root),
            vec![FormRow::Name, FormRow::AddButton]
        );
    }

    #[test]
    fn edit_form_rows_are_name_fields_and_add_button() {
        let mut node = Node::new("n");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: String::new(),
        });
        let state = TreeState::new(node);
        let form = FormState::new_edit(&state.root);
        assert_eq!(
            form.rows(&state.root),
            vec![FormRow::Name, FormRow::Extra(0), FormRow::AddButton]
        );
    }

    #[test]
    fn add_form_first_commit_creates_the_node() {
        let mut state = TreeState::new(Node::new("Root"));
        state.add_position = AddPosition::Child;
        state.mode = InputMode::Form(FormState::new_add());
        assert!(form_of(&state).is_adding());

        handle_form(key(KeyCode::Char('i')), &mut state);
        type_text(&mut state, "Child");
        handle_form(key(KeyCode::Enter), &mut state);

        let form = form_of(&state);
        assert!(!form.is_adding());
        let id = form.node_id.expect("node created");
        let created = state.root.find(id).expect("node in tree");
        assert_eq!(created.name, "Child");
        assert_eq!(state.visible[state.selected].id, id);
        assert!(state.modified);
    }

    #[test]
    fn adding_field_via_picker_creates_node_and_enters_input() {
        let mut state = TreeState::new(Node::new("Root"));
        state.add_position = AddPosition::Child;
        state.mode = InputMode::Form(FormState::new_add());
        select_row(&mut state, 1); // the + Add Field row

        handle_form(key(KeyCode::Enter), &mut state);
        assert!(matches!(state.mode, InputMode::SelectFieldType { .. }));

        // Custom is offered last; Description sits before it.
        let items = match &state.mode {
            InputMode::SelectFieldType { form, .. } => picker_items(form),
            _ => unreachable!(),
        };
        assert_eq!(items.last(), Some(&PickerItem::Custom));

        let desc_index = items
            .iter()
            .position(|item| *item == PickerItem::Desc)
            .expect("Desc offered");
        if let InputMode::SelectFieldType {
            ref mut selected, ..
        } = state.mode
        {
            *selected = desc_index;
        }
        handle_select_field_type(key(KeyCode::Enter), &mut state);

        let form = form_of(&state);
        assert!(!form.is_adding()); // the first commit created the node
        assert_eq!(form.focus, FormFocus::Input);
        assert_eq!(form.current_row(&state.root), FormRow::Extra(0));
        let node = form_node(&state);
        assert_eq!(node.extra_fields.len(), 1);
        assert_eq!(node.extra_fields[0].field_type.label(), "Description");
        assert_eq!(node.extra_fields[0].value, "");
        assert!(state.modified);
    }

    #[test]
    fn password_is_masked_until_input_level() {
        let ef = ExtraField {
            field_type: FieldType::Password,
            value: "secret".to_string(),
        };
        // Nav: masked.
        let line = render_extra_field_line(&ef, &ef.value, 0, true, false);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!text.contains("secret"));
        assert!(text.contains(texts::PASSWORD_MASK));

        // Input: revealed.
        let line = render_extra_field_line(&ef, &ef.value, 0, true, true);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("secret"));
    }

    #[test]
    fn field_cut_requires_two_d_presses_and_is_live() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1); // "a"

        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::Cut);
        assert_eq!(values(&state).len(), 3); // not removed yet

        handle_form(key(KeyCode::Char('d')), &mut state);
        let form = form_of(&state);
        assert_eq!(form.pending, FormPending::None);
        assert_eq!(values(&state), vec!["b", "c"]);
        assert_eq!(form.field_cut_buffer.as_ref().unwrap().value, "a");
    }

    #[test]
    fn field_cut_is_cancelled_by_other_keys() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);

        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Esc), &mut state);
        let form = form_of(&state);
        assert_eq!(form.pending, FormPending::None);
        assert_eq!(values(&state).len(), 3);
        assert!(matches!(state.mode, InputMode::Form(_)));
    }

    #[test]
    fn field_cut_is_noop_on_name_row() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 0);
        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::None);
    }

    #[test]
    fn field_paste_after_moves_field_down() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1); // "a"

        // Cut "a", then select "b" and paste after it.
        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        select_row(&mut state, 1); // "b"
        handle_form(key(KeyCode::Char('p')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::Paste);
        handle_form(key(KeyCode::Char('p')), &mut state);

        let form = form_of(&state);
        assert_eq!(values(&state), vec!["b", "a", "c"]);
        assert!(form.field_cut_buffer.is_none());
    }

    #[test]
    fn undo_clears_the_field_cut_buffer() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        assert!(form_of(&state).field_cut_buffer.is_some());

        handle_form(key(KeyCode::Char('u')), &mut state);
        assert!(form_of(&state).field_cut_buffer.is_none());
        assert_eq!(values(&state), vec!["a", "b", "c"]);
    }

    #[test]
    fn paste_without_buffer_does_nothing() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        handle_form(key(KeyCode::Char('p')), &mut state);
        assert_eq!(form_of(&state).pending, FormPending::None);
        assert_eq!(values(&state).len(), 3);
    }

    #[test]
    fn p_with_empty_buffer_stays_in_normal() {
        let mut state = TreeState::new(Node::new("Root"));
        handle_normal(key(KeyCode::Char('p')), &mut state);
        assert!(matches!(state.mode, InputMode::Normal));
    }

    #[test]
    fn p_with_buffer_enters_pending_paste() {
        let mut state = TreeState::new(Node::new("Root"));
        state.cut_buffer = Some(Node::new("X"));
        handle_normal(key(KeyCode::Char('p')), &mut state);
        assert!(matches!(state.mode, InputMode::PendingPaste));
    }

    #[test]
    fn normal_hint_advertises_paste_only_with_a_cut_buffer() {
        let mut state = TreeState::new(Node::new("Root"));
        assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE);

        state.cut_buffer = Some(Node::new("X"));
        assert_eq!(status_bar_parts(&state).2, texts::HINT_TREE_PASTE);
    }

    #[test]
    fn confirm_delete_removes_the_field_and_clamps_selection() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 2); // second extra field
        state.mode = match std::mem::replace(&mut state.mode, InputMode::Normal) {
            InputMode::Form(form) => InputMode::ConfirmDeleteField {
                form,
                target_index: 1,
            },
            _ => unreachable!(),
        };
        handle_confirm_delete(key(KeyCode::Char('y')), &mut state);
        assert_eq!(values(&state), vec!["a", "c"]);
        assert!(form_of(&state).selected_row < form_of(&state).rows(&state.root).len());
    }

    /// A tree whose root has three extra fields, opened in the Detail overlay.
    fn detail_state() -> TreeState {
        let mut node = Node::new("Root");
        for value in ["a", "b", "c"] {
            node.extra_fields.push(ExtraField {
                field_type: FieldType::Login,
                value: value.to_string(),
            });
        }
        let mut state = TreeState::new(node);
        state.mode = InputMode::Detail {
            selected: 0,
            alert: None,
        };
        state
    }

    fn detail_selected(state: &TreeState) -> usize {
        match &state.mode {
            InputMode::Detail { selected, .. } => *selected,
            other => panic!("expected Detail mode, got {other:?}"),
        }
    }

    #[test]
    fn detail_tab_wraps_forward_and_backtab_wraps_backward() {
        let mut state = detail_state();
        handle_detail(key(KeyCode::Tab), &mut state);
        assert_eq!(detail_selected(&state), 1);
        handle_detail(key(KeyCode::Tab), &mut state);
        handle_detail(key(KeyCode::Tab), &mut state);
        assert_eq!(detail_selected(&state), 0); // wrapped past the last field

        handle_detail(key(KeyCode::BackTab), &mut state);
        assert_eq!(detail_selected(&state), 2); // wrapped past the first field
    }

    #[test]
    fn detail_nav_down_clamps_at_last_field() {
        let mut state = detail_state();
        for _ in 0..5 {
            handle_detail(key(KeyCode::Char('j')), &mut state);
        }
        assert_eq!(detail_selected(&state), 2); // last field, no phantom row
    }

    #[test]
    fn detail_nav_up_clamps_at_first_field() {
        let mut state = detail_state();
        handle_detail(key(KeyCode::Char('k')), &mut state);
        assert_eq!(detail_selected(&state), 0);
    }

    #[test]
    fn detail_tab_is_safe_without_fields() {
        let mut state = TreeState::new(Node::new("Root"));
        state.mode = InputMode::Detail {
            selected: 0,
            alert: None,
        };
        handle_detail(key(KeyCode::Tab), &mut state);
        handle_detail(key(KeyCode::BackTab), &mut state);
        assert_eq!(detail_selected(&state), 0);
    }

    #[test]
    fn detail_y_sets_copy_alert() {
        // An empty value skips the clipboard subprocess but still alerts.
        let mut node = Node::new("Root");
        node.extra_fields.push(ExtraField {
            field_type: FieldType::Login,
            value: String::new(),
        });
        let mut state = TreeState::new(node);
        state.mode = InputMode::Detail {
            selected: 0,
            alert: None,
        };
        handle_detail(key(KeyCode::Char('y')), &mut state);
        match &state.mode {
            InputMode::Detail { alert, .. } => {
                assert_eq!(alert.as_deref(), Some(texts::COPY_CHECK));
            }
            other => panic!("expected Detail mode, got {other:?}"),
        }
    }

    #[test]
    fn start_autosave_without_a_target_returns_none() {
        let (tx, _rx) = mpsc::unbounded_channel();
        assert!(start_autosave(None, tx).unwrap().is_none());
    }

    #[test]
    fn runtime_supports_bounded_timeouts() {
        let runtime = save_runtime().unwrap();
        let timed_out = runtime.block_on(async {
            tokio::time::timeout(
                std::time::Duration::from_millis(1),
                std::future::pending::<()>(),
            )
            .await
        });

        assert!(timed_out.is_err());
    }

    #[test]
    fn exit_plan_resubmits_after_a_failed_save() {
        let save_state = SaveState::Failed {
            error: failed("disk full"),
            acknowledged: false,
        };
        assert_eq!(
            exit_plan(false, &save_state, 1, 2),
            ExitPlan::ResubmitThenWait
        );
    }

    #[test]
    fn exit_plan_waits_for_an_in_flight_snapshot() {
        assert_eq!(
            exit_plan(false, &SaveState::Saving, 1, 2),
            ExitPlan::Wait(2)
        );
    }

    #[test]
    fn exit_plan_is_up_to_date_when_everything_is_written() {
        assert_eq!(exit_plan(false, &SaveState::Idle, 2, 2), ExitPlan::UpToDate);
    }

    #[test]
    fn exit_plan_resubmits_when_a_commit_is_unsubmitted() {
        assert_eq!(
            exit_plan(true, &SaveState::Idle, 2, 2),
            ExitPlan::ResubmitThenWait
        );
    }

    #[test]
    fn blocked_overlay_retries_and_keeps_the_exit_request() {
        let mut state = TreeState::new(Node::new("Root"));
        state.save_exit = SaveExit::Blocked;
        state.quit = true;

        let action = handle_save_overlay(key(KeyCode::Char('r')), &mut state);

        assert!(matches!(action, OverlayAction::Retry));
        assert_eq!(state.save_exit, SaveExit::None);
        assert!(state.quit);
    }

    #[test]
    fn blocked_overlay_confirms_then_discards() {
        let mut state = TreeState::new(Node::new("Root"));
        state.save_exit = SaveExit::Blocked;
        state.return_to_menu = true;

        let action = handle_save_overlay(key(KeyCode::Char('q')), &mut state);
        assert!(matches!(action, OverlayAction::Stay));
        assert_eq!(state.save_exit, SaveExit::ConfirmDiscard);

        let action = handle_save_overlay(key(KeyCode::Char('y')), &mut state);
        assert!(matches!(action, OverlayAction::Discard));
        assert!(state.return_to_menu);
    }

    #[test]
    fn blocked_overlay_other_key_cancels_the_exit() {
        let mut state = TreeState::new(Node::new("Root"));
        state.save_exit = SaveExit::Blocked;
        state.quit = true;

        let action = handle_save_overlay(key(KeyCode::Char('x')), &mut state);

        assert!(matches!(action, OverlayAction::Stay));
        assert_eq!(state.save_exit, SaveExit::None);
        assert!(!state.quit);
    }

    #[tokio::test]
    async fn wait_for_save_reports_the_target_generation() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut saved = 0;

        tx.send(SaveEvent {
            generation: 1,
            result: Ok(()),
        })
        .unwrap();
        tx.send(SaveEvent {
            generation: 3,
            result: Ok(()),
        })
        .unwrap();

        let result = wait_for_generation(&mut rx, &mut saved, 3).await;

        assert!(result.is_ok());
        assert_eq!(saved, 3);
    }

    #[tokio::test]
    async fn wait_for_save_keeps_the_last_successful_generation_on_error() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut saved = 0;

        tx.send(SaveEvent {
            generation: 1,
            result: Ok(()),
        })
        .unwrap();
        tx.send(SaveEvent {
            generation: 2,
            result: Err(failed("disk full")),
        })
        .unwrap();

        let result = wait_for_generation(&mut rx, &mut saved, 2).await;

        assert!(matches!(result, Err(SaveError::Failed(_))));
        assert_eq!(saved, 1);
    }

    #[tokio::test]
    async fn wait_for_save_detects_a_stopped_worker() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        drop(tx);
        let mut saved = 0;

        let result = wait_for_generation(&mut rx, &mut saved, 1).await;

        assert!(matches!(result, Err(SaveError::WorkerStopped)));
    }

    #[test]
    fn generator_ctrl_g_is_bound_only_while_editing_a_password_field() {
        // `Ctrl+G` is unbound at the Nav level, even on a password row.
        let mut nav = editing_state_with_password("old");
        select_row(&mut nav, 1);
        handle_form(ctrl(KeyCode::Char('g')), &mut nav);
        assert!(matches!(nav.mode, InputMode::Form(_)));

        // At the Input level it opens only when the focused field carries a
        // generator: Login and Name rows are no-ops.
        let mut login = editing_state_with_fields();
        select_row(&mut login, 1);
        handle_form(key(KeyCode::Char('i')), &mut login);
        handle_form(ctrl(KeyCode::Char('g')), &mut login);
        assert!(matches!(login.mode, InputMode::Form(_)));

        let mut name = editing_state();
        select_row(&mut name, 0);
        handle_form(key(KeyCode::Char('i')), &mut name);
        handle_form(ctrl(KeyCode::Char('g')), &mut name);
        assert!(matches!(name.mode, InputMode::Form(_)));

        let mut password = editing_state_with_password("old");
        open_generator(&mut password);
    }

    #[test]
    fn generator_generate_writes_commits_and_returns_to_input_form() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        set_generator_length(&mut state, "12");
        press_generate(&mut state);

        assert!(matches!(state.mode, InputMode::Form(_)));
        assert_eq!(form_of(&state).focus, FormFocus::Input);
        let value = password_value(&state);
        assert_eq!(value.chars().count(), 12);
        assert!(value.chars().all(|c| c.is_ascii_graphic()));
        assert!(state.modified);
        assert_eq!(form_of(&state).history.len(), 2);
        assert_eq!(state.generator.length.value(), 12);

        // The scratch buffer mirrors the generated value, so leaving the input
        // level keeps the password instead of restoring the old one.
        let generated = value.to_string();
        assert_eq!(
            form_of(&state).scratch.as_ref().map(|s| s.text.as_str()),
            Some(generated.as_str())
        );
        handle_form(key(KeyCode::Esc), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        assert_eq!(password_value(&state), generated);
    }

    #[test]
    fn generated_password_is_undoable() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        press_generate(&mut state);
        assert_ne!(password_value(&state), "old");

        // Generation leaves the form at the Input level, where `u` is plain
        // text; leave it first so `u` reaches the Nav-level undo.
        handle_form(key(KeyCode::Esc), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(password_value(&state), "old");
    }

    #[test]
    fn generator_esc_leaves_the_value_untouched() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        dispatch_key(key(KeyCode::Esc), &mut state);

        assert!(matches!(state.mode, InputMode::Form(_)));
        assert_eq!(password_value(&state), "old");
        assert!(!state.modified);
        assert_eq!(form_of(&state).history.len(), 1);
    }

    #[test]
    fn generator_out_of_range_length_shows_an_error_and_does_not_write() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        set_generator_length(&mut state, "999");
        press_generate(&mut state);

        assert!(matches!(state.mode, InputMode::GeneratePassword(_)));
        assert!(generator_modal(&state).error.is_some());
        let value = &generator_modal(&state).form.node(&state.root).extra_fields[0].value;
        assert_eq!(value, "old");
        assert!(!state.modified);
    }

    #[test]
    fn generator_digits_replace_the_seed_and_edit_at_the_caret() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);

        // The seeded "10" is replaced by the first digits typed.
        dispatch_key(key(KeyCode::Char('1')), &mut state);
        dispatch_key(key(KeyCode::Char('2')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "12");
        assert_eq!(generator_modal(&state).length_cursor, 2);

        // The caret can be moved and a digit inserted mid-string.
        dispatch_key(key(KeyCode::Left), &mut state);
        assert_eq!(generator_modal(&state).length_cursor, 1);
        dispatch_key(key(KeyCode::Char('0')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "102");

        // Home / End move to the edges; Delete removes at the caret.
        dispatch_key(key(KeyCode::Home), &mut state);
        assert_eq!(generator_modal(&state).length_cursor, 0);
        dispatch_key(key(KeyCode::Delete), &mut state);
        assert_eq!(generator_modal(&state).length_input, "02");
        dispatch_key(key(KeyCode::End), &mut state);
        assert_eq!(generator_modal(&state).length_cursor, 2);

        // At most three digits are kept.
        dispatch_key(key(KeyCode::Char('7')), &mut state);
        dispatch_key(key(KeyCode::Char('8')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "027");
    }

    #[test]
    fn generator_length_backspace_on_fresh_seed_clears_it() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);

        dispatch_key(key(KeyCode::Backspace), &mut state);
        assert_eq!(generator_modal(&state).length_input, "");
        assert_eq!(generator_modal(&state).length_cursor, 0);

        dispatch_key(key(KeyCode::Char('4')), &mut state);
        dispatch_key(key(KeyCode::Char('2')), &mut state);
        assert_eq!(generator_modal(&state).length_input, "42");
    }

    #[test]
    fn generator_digits_and_caret_keys_only_edit_the_length_row() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_GENERATE);

        for code in [
            KeyCode::Char('5'),
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::Left,
            KeyCode::Home,
        ] {
            dispatch_key(key(code), &mut state);
        }
        assert_eq!(generator_modal(&state).length_input, "10");
        assert_eq!(generator_modal(&state).length_cursor, 2);
    }

    #[test]
    fn generator_enter_on_length_moves_to_symbols_row() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_LENGTH);

        dispatch_key(key(KeyCode::Enter), &mut state);
        assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_SYMBOLS);
    }

    #[test]
    fn generator_symbols_summary_toggles_all() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);

        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert!(generator_modal(&state).params.symbols.is_empty());
        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert_eq!(
            generator_modal(&state).params.symbols,
            generator::SymbolSet::all()
        );
    }

    #[test]
    fn symbol_checklist_moves_in_a_grid_and_toggles() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);
        dispatch_key(key(KeyCode::Enter), &mut state);
        assert!(matches!(state.mode, InputMode::SymbolChecklist(_)));

        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert!(!generator_modal(&state).params.symbols.contains('!'));
        dispatch_key(key(KeyCode::Char('l')), &mut state);
        assert_eq!(generator_modal(&state).selected, 1);
        dispatch_key(key(KeyCode::Char('j')), &mut state);
        assert_eq!(generator_modal(&state).selected, 4);
        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        assert!(!generator_modal(&state).params.symbols.contains('%'));

        dispatch_key(key(KeyCode::Esc), &mut state);
        assert!(matches!(state.mode, InputMode::GeneratePassword(_)));
        assert_eq!(generator_modal(&state).selected, GENERATOR_ROW_SYMBOLS);
    }

    #[test]
    fn generator_with_all_symbols_off_generates_alphanumerics() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        move_generator_to(&mut state, GENERATOR_ROW_SYMBOLS);
        dispatch_key(key(KeyCode::Char(' ')), &mut state);
        press_generate(&mut state);

        assert!(matches!(state.mode, InputMode::Form(_)));
        let value = password_value(&state);
        assert!(value.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(value.chars().any(|c| c.is_ascii_digit()));
    }

    #[test]
    fn generator_settings_are_remembered_for_the_next_open() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        set_generator_length(&mut state, "12");
        press_generate(&mut state);

        open_generator(&mut state);
        assert_eq!(generator_modal(&state).length_input, "12");
    }

    #[test]
    fn form_hint_offers_the_generator_only_while_editing_a_generated_field() {
        let mut password = editing_state_with_password("old");
        select_row(&mut password, 1);
        assert_eq!(
            form_hint(form_of(&password), &password.root),
            texts::HINT_FORM_EXTRA
        );
        handle_form(key(KeyCode::Char('i')), &mut password);
        assert_eq!(
            form_hint(form_of(&password), &password.root),
            texts::HINT_FORM_GENERATOR_INPUT
        );

        let mut login = editing_state_with_fields();
        select_row(&mut login, 1);
        assert_eq!(
            form_hint(form_of(&login), &login.root),
            texts::HINT_FORM_EXTRA
        );
        handle_form(key(KeyCode::Char('i')), &mut login);
        assert_eq!(
            form_hint(form_of(&login), &login.root),
            texts::HINT_FORM_GENERATOR_INPUT
        );

        // A Custom field has no generator, so its hint stays the plain input
        // one at the Input level.
        let mut custom = editing_state();
        custom.root.extra_fields.push(ExtraField {
            field_type: FieldType::Custom("URL".to_string()),
            value: String::new(),
        });
        let node = custom.root.clone();
        custom.mode = InputMode::Form(FormState::new_edit(&node));
        select_row(&mut custom, 1);
        handle_form(key(KeyCode::Char('i')), &mut custom);
        assert_eq!(
            form_hint(form_of(&custom), &custom.root),
            texts::HINT_FORM_INPUT
        );

        let mut name = editing_state();
        select_row(&mut name, 0);
        assert_eq!(form_hint(form_of(&name), &name.root), texts::HINT_FORM);
    }

    #[test]
    fn form_extra_hint_advertises_paste_only_with_a_field_buffer() {
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        assert_eq!(
            form_hint(form_of(&state), &state.root),
            texts::HINT_FORM_EXTRA
        );

        handle_form(key(KeyCode::Char('d')), &mut state);
        handle_form(key(KeyCode::Char('d')), &mut state);
        assert_eq!(
            form_hint(form_of(&state), &state.root),
            texts::HINT_FORM_EXTRA_PASTE
        );
    }

    fn open_login_row(state: &mut TreeState) {
        select_row(state, 1);
        if form_of(state).focus == FormFocus::Nav {
            handle_form(key(KeyCode::Char('i')), state);
        }
    }

    fn generate_login_word(state: &mut TreeState) {
        assert_eq!(form_of(state).focus, FormFocus::Input);
        handle_form(ctrl(KeyCode::Char('g')), state);
    }

    #[test]
    fn ctrl_g_on_a_login_row_writes_a_word_without_opening_a_modal() {
        let mut state = editing_state_with_fields();
        open_login_row(&mut state);
        generate_login_word(&mut state);

        // No modal: the form is still active at the Input level.
        assert!(matches!(state.mode, InputMode::Form(_)));
        assert_eq!(form_of(&state).focus, FormFocus::Input);

        let value = values(&state)[0];
        assert_ne!(value, "a");
        assert!(value.chars().all(|c| c.is_ascii_lowercase()));
        assert!((4..=13).contains(&value.chars().count()));
        assert!(state.modified);
        // The scratch buffer mirrors the value, so leaving Input keeps it.
        assert_eq!(
            form_of(&state).scratch.as_ref().map(|s| s.text.as_str()),
            Some(value)
        );
        let generated = value.to_string();
        handle_form(key(KeyCode::Esc), &mut state);
        assert_eq!(values(&state)[0], generated);
    }

    #[test]
    fn login_reroll_coalesces_into_one_undo_step() {
        let mut state = editing_state_with_fields();
        open_login_row(&mut state);
        generate_login_word(&mut state);
        assert_eq!(form_of(&state).history.len(), 2);

        generate_login_word(&mut state);
        // The reroll replaced the previous generation instead of stacking.
        assert_eq!(form_of(&state).history.len(), 2);
        assert!(values(&state)[0].chars().all(|c| c.is_ascii_lowercase()));

        // One undo restores the pre-generation value.
        handle_form(key(KeyCode::Esc), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(values(&state)[0], "a");
    }

    #[test]
    fn ctrl_g_outside_a_generated_field_is_a_noop() {
        // Name row at the Input level.
        let mut state = editing_state();
        select_row(&mut state, 0);
        handle_form(key(KeyCode::Char('i')), &mut state);
        handle_form(ctrl(KeyCode::Char('g')), &mut state);
        assert!(!state.modified);
        assert_eq!(form_of(&state).history.len(), 1);

        // A Login row at the Nav level: `Ctrl+G` is unbound there.
        let mut state = editing_state_with_fields();
        select_row(&mut state, 1);
        handle_form(ctrl(KeyCode::Char('g')), &mut state);
        assert_eq!(form_of(&state).focus, FormFocus::Nav);
        assert!(!state.modified);
        assert_eq!(values(&state)[0], "a");
    }

    #[test]
    fn password_reroll_also_coalesces_into_one_undo_step() {
        let mut state = editing_state_with_password("old");
        open_generator(&mut state);
        press_generate(&mut state);
        assert_eq!(form_of(&state).history.len(), 2);

        open_generator(&mut state);
        press_generate(&mut state);
        assert_eq!(form_of(&state).history.len(), 2);

        handle_form(key(KeyCode::Esc), &mut state);
        handle_form(key(KeyCode::Char('u')), &mut state);
        assert_eq!(password_value(&state), "old");
    }

    /// Renders `ui` into a test backend and asserts that no cell falls through
    /// to the terminal's own background (see
    /// [ADR 0018](../../../../docs/adr/0018-app-owned-background-fill.md)).
    fn assert_no_terminal_background(state: &mut TreeState) {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| ui(frame, state)).unwrap();

        let buffer = terminal.backend().buffer();
        for (i, cell) in buffer.content.iter().enumerate() {
            assert_ne!(
                cell.style().bg,
                Some(Color::Reset),
                "cell {i} ({:?}) fell through to the terminal background",
                cell.symbol()
            );
        }
    }

    #[test]
    fn tree_screen_paints_every_cell() {
        let mut state = TreeState::new(crate::build_demo_tree());
        assert_no_terminal_background(&mut state);
    }

    #[test]
    fn search_overlay_paints_every_cell() {
        // The Search overlay clears its rect before drawing the block, the path
        // most likely to expose the terminal background.
        let mut state = TreeState::new(Node::new("Root"));
        state.mode = InputMode::Search;
        state.search_query = "Ro".to_string();
        assert_no_terminal_background(&mut state);
    }
}
