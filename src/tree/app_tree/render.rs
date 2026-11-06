//! Drawing the Tree screen: the frame renderer, the status bar and the overlays.
//!
//! [`ui`] paints the app-owned background, draws the tree through
//! [`TreeWidget::render_with_hits`] (recording the click regions), draws the
//! unified status bar and any modal on top, and finally installs the overlays
//! (search, save error, discard confirmation).

use std::borrow::Cow;

use crate::theming::{ElementStyle, paint_background, render_status_bar, texts};
use crate::tree::widget::{InputMode, Modal, SaveExit, TreeState, TreeWidget, centered_rect};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

pub(super) fn ui(frame: &mut Frame, state: &mut TreeState) {
    let area = frame.area();

    // Own the background: paint SURFACE across the whole screen first so the
    // terminal's own background never shows through.
    paint_background(frame, area);

    let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

    // Take the hit map out for the frame, refill it, and put it back: this keeps
    // the renderer's `&TreeState` borrow disjoint from the map, and a click next
    // iteration reads the regions this frame drew.
    let mut hits = std::mem::take(&mut state.hits);
    hits.clear();
    TreeWidget::render_with_hits(frame, layout[0], state, &mut hits);

    let (mode_str, mode_style, hint) = status_bar_parts(state);
    render_status_bar(
        frame,
        layout[1],
        mode_str,
        mode_style.style(),
        hint.as_ref(),
    );

    if let InputMode::Search = &state.mode {
        render_search_overlay(frame, area, state);
    }

    // The modal is taken out for drawing: the browser's renderer needs `&mut` for
    // its viewport offsets, while the Detail modal reads the tree behind it.
    if let Some(mut modal) = state.modal.take() {
        match &mut modal {
            Modal::Help(help) => help.render(frame, area),
            Modal::Alert(alert) => alert.render(frame, area),
            Modal::Detail(detail) => detail.render(frame, area, state),
            Modal::Export(wizard) => wizard.render(frame, area),
            Modal::ExportDestination(destination) => destination.render(frame, area),
            Modal::Share(wizard) => wizard.render(frame, area),
            Modal::ShareDestination(destination) => destination.render(frame, area),
            Modal::SharePassphrase(prompt) => prompt.render(frame, area),
        }
        state.modal = Some(modal);
    }

    match state.save_exit {
        SaveExit::ConfirmDiscard => render_discard_confirm_overlay(frame, area),
        _ => {
            if let Some(message) = state.save_state.overlay_message() {
                render_save_error_overlay(frame, area, &message);
            }
        }
    }

    state.hits = hits;
}

/// Maps the active tree mode (or a parked save overlay) to the unified
/// status-bar parts: `(mode label, badge style, context hint)`.
///
/// A parked save overlay takes priority over the mode because it is drawn on
/// top of it. It carries no badge: it is a transient prompt, not a mode.
pub(super) fn status_bar_parts(state: &TreeState) -> (&'static str, ElementStyle, Cow<'_, str>) {
    match state.save_exit {
        SaveExit::ConfirmDiscard => {
            return (
                "",
                ElementStyle::ModeNormal,
                Cow::Borrowed(texts::HINT_DISCARD_CONFIRM),
            );
        }
        SaveExit::Blocked => {
            return (
                "",
                ElementStyle::ModeNormal,
                Cow::Borrowed(texts::HINT_SAVE_EXIT_BLOCKED),
            );
        }
        SaveExit::None => {}
    }
    if state.save_state.overlay_message().is_some() {
        return (
            "",
            ElementStyle::ModeNormal,
            Cow::Borrowed(texts::HINT_DETAIL_ALERT),
        );
    }

    if let Some(modal) = &state.modal {
        return match modal {
            Modal::Help(help) => {
                let (mode, style, hint) = help.status_parts();
                (mode, style, Cow::Borrowed(hint))
            }
            Modal::Alert(alert) => {
                let (mode, style, hint) = alert.status_parts();
                (mode, style, Cow::Borrowed(hint))
            }
            Modal::Detail(detail) => {
                let (mode, style, hint) = detail.status_parts(&state.root);
                (mode, style, Cow::Borrowed(hint))
            }
            Modal::Export(wizard) => {
                let (mode, style, hint) = wizard.status_parts();
                (mode, style, Cow::Borrowed(hint))
            }
            // The File Browser owns its badge and hint — they follow its mode and
            // the overlay it is showing — so its parts pass through unchanged.
            Modal::ExportDestination(destination) => {
                let (mode, style, hint) = destination.status_parts();
                (mode, style, Cow::Owned(hint))
            }
            Modal::Share(wizard) => {
                let (mode, style, hint) = wizard.status_parts();
                (mode, style, Cow::Borrowed(hint))
            }
            Modal::ShareDestination(destination) => {
                let (mode, style, hint) = destination.status_parts();
                (mode, style, Cow::Owned(hint))
            }
            Modal::SharePassphrase(prompt) => {
                let (mode, style, hint) = prompt.status_parts();
                (mode, style, Cow::Borrowed(hint))
            }
        };
    }

    let (mode, style, hint) = match &state.mode {
        InputMode::Normal => (
            texts::MODE_NAV,
            ElementStyle::ModeNormal,
            // The Root offers no `d:cut`, so it picks from the root variants.
            match (state.is_root_selected(), state.cut_buffer.is_some()) {
                (false, false) => texts::HINT_TREE,
                (false, true) => texts::HINT_TREE_PASTE,
                (true, false) => texts::HINT_TREE_ROOT,
                (true, true) => texts::HINT_TREE_ROOT_PASTE,
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
            if state.is_root_selected() {
                texts::HINT_PASTE_ROOT
            } else {
                texts::HINT_PASTE
            },
        ),
        InputMode::PendingAdd => (
            texts::MODE_PENDING_ADD,
            ElementStyle::ModePendingAdd,
            if state.is_root_selected() {
                texts::HINT_PENDING_ADD_ROOT
            } else {
                texts::HINT_PENDING_ADD
            },
        ),
        InputMode::PendingExport => (
            texts::MODE_PENDING_EXPORT,
            ElementStyle::ModePendingExport,
            texts::HINT_PENDING_EXPORT,
        ),
    };
    (mode, style, Cow::Borrowed(hint))
}

fn render_search_overlay(frame: &mut Frame, area: Rect, state: &TreeState) {
    let overlay_area = centered_rect(area, area.width / 2, 3);

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
    let overlay_area = centered_rect(area, area.width * 2 / 3, 4);

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
