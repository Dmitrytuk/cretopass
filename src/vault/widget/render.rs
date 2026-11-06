//! Rendering the Home screen and its modal.
//!
//! The screen paints its own background, draws the menu (or whatever modal is
//! open over it) and routes the status bar for the active modal.

use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theming::{ElementStyle, Icon, paint_background, render_status_bar, texts};

use super::modal::Modal;
use super::state::{HomeHit, HomeState}; // ── Rendering ──
use crate::hitbox::record_rows;

pub fn render(frame: &mut Frame, area: Rect, state: &mut HomeState) {
    // Own the background: paint SURFACE across the whole screen first so the
    // terminal's own background never shows through.
    paint_background(frame, area);

    state.hits.clear();
    render_home_window(frame, area, state);

    // The modal is taken out for the frame: its renderer keeps its own viewport
    // offsets (the File Browser) and records its own clickable rows, so it needs
    // mutable access.
    let mut modal = state.modal.take();
    if let Some(Modal::FileBrowser(browser)) = &mut modal {
        browser.render(frame, area);
        state.modal = modal;
        return;
    }

    match &mut modal {
        // The File Browser returned above; this arm only satisfies exhaustiveness.
        Some(Modal::FileBrowser(_)) => unreachable!("the File Browser returned above"),
        Some(Modal::Alert(alert)) => alert.render(frame, area),
        Some(Modal::About(about)) => about.render(frame, area),
        Some(Modal::Confirmation(confirmation)) => confirmation.render(frame, area),
        Some(Modal::Unlock(prompt)) => prompt.render(frame, area, state.config.display_full_path),
        Some(Modal::CreateSecret(prompt)) => {
            prompt.render(frame, area, state.config.display_full_path)
        }
        Some(Modal::Configuration(modal)) => modal.render(frame, area, &state.config),
        Some(Modal::ChangePassword(prompt)) => prompt.render(frame, area),
        Some(Modal::Export(wizard)) => wizard.render(frame, area),
        Some(Modal::SharePassphrase(prompt)) => {
            prompt.render(frame, area, state.config.display_full_path)
        }
        None => {}
    }
    state.modal = modal;
}

pub(super) fn render_home_window(frame: &mut Frame, area: Rect, state: &mut HomeState) {
    let layout = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);

    let base_title = state
        .active_vault
        .as_ref()
        .and_then(|af| af.path.as_deref())
        .map(|p| {
            if state.config.display_full_path {
                format!("{} {} ", texts::TITLE_HOME_PREFIX, p)
            } else {
                let name = std::path::Path::new(p)
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or(std::borrow::Cow::Borrowed(p));
                format!("{} {} ", texts::TITLE_HOME_PREFIX, name)
            }
        })
        .unwrap_or_else(|| texts::TITLE_HOME_DEFAULT.to_string());

    let mut title_spans: Vec<Span> = Vec::new();
    if let Some(status) = state.status {
        let mut text = String::new();
        if status.spins() {
            text.push_str(state.spinner.glyph());
            text.push(' ');
        }
        text.push_str(status.label());
        title_spans.push(Span::styled(format!(" {text}  "), status.style().style()));
    }
    title_spans.push(Span::raw(base_title));

    let block = Block::default()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_style(ElementStyle::HomeBlockBorder.style());

    let inner = block.inner(layout[0]);
    frame.render_widget(block, layout[0]);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, item) in texts::MENU_ITEMS.iter().enumerate() {
        let is_selected = state.selected == i;
        let style = if is_selected {
            ElementStyle::HomeSelected.style()
        } else {
            ElementStyle::HomeNormal.style()
        };
        let prefix = if is_selected {
            Icon::SelectionArrow.as_str()
        } else {
            Icon::SelectionPad.as_str()
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, item),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        texts::SEPARATOR_RECENT_FILES,
        ElementStyle::HomeSectionHeader.style(),
    )));
    lines.push(Line::from(""));

    let menu_len = texts::MENU_ITEMS.len();
    for (i, file) in state.config.recent_files.iter().enumerate() {
        let idx = menu_len + i;
        let is_selected = state.selected == idx;
        let style = if is_selected {
            ElementStyle::HomeSelected.style()
        } else {
            ElementStyle::HomeNormal.style()
        };
        let prefix = if is_selected {
            Icon::SelectionArrow.as_str()
        } else {
            Icon::SelectionPad.as_str()
        };
        let display = if state.config.display_full_path {
            file.clone()
        } else {
            std::path::Path::new(file)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file.clone())
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, display),
            style,
        )));
    }

    // Record the menu and recent rows. They sit one blank line below the block's
    // top: menu item `i` on line `1 + i`, and the recents after a blank, the
    // section header, and another blank. Only rows inside the drawn window can
    // be clicked.
    let menu_len = texts::MENU_ITEMS.len();
    let visible_menu = menu_len.min(inner.height.saturating_sub(1) as usize);
    record_rows(&mut state.hits, inner, 1, visible_menu, HomeHit::MenuItem);
    let recent_rows = inner
        .height
        .saturating_sub(4 + u16::try_from(menu_len).unwrap_or(u16::MAX));
    let visible_recent = state.config.recent_files.len().min(recent_rows as usize);
    record_rows(
        &mut state.hits,
        inner,
        4 + menu_len as u16,
        visible_recent,
        |j| HomeHit::MenuItem(menu_len + j),
    );

    frame.render_widget(Paragraph::new(Text::from(lines)), inner);

    let (mode_str, mode_style, hint) = home_status_parts(state);
    render_status_bar(frame, layout[1], mode_str, mode_style.style(), &hint);
}

/// Maps the active Home modal (or the menu) to the unified status-bar parts:
/// `(mode label, badge style, context hint)`.
///
/// `Alert` has no badge: it is a transient dismissal prompt, not a mode. When
/// the File Browser is open, the badge and hint mirror the browser's own status
/// bar so the two never disagree.
pub(super) fn home_status_parts(state: &HomeState) -> (&'static str, ElementStyle, Cow<'_, str>) {
    match &state.modal {
        None => (
            texts::MODE_MENU,
            ElementStyle::ModeNormal,
            Cow::Borrowed(texts::HINT_HOME),
        ),
        Some(Modal::FileBrowser(browser)) => browser.status_parts(),
        Some(Modal::Unlock(prompt)) => prompt.status_parts(),
        Some(Modal::CreateSecret(prompt)) => prompt.status_parts(),
        Some(Modal::Configuration(modal)) => modal.status_parts(),
        Some(Modal::Alert(alert)) => alert.status_parts(),
        Some(Modal::Confirmation(confirmation)) => confirmation.status_parts(),
        Some(Modal::ChangePassword(prompt)) => prompt.status_parts(),
        Some(Modal::Export(wizard)) => wizard.status_parts(),
        Some(Modal::SharePassphrase(prompt)) => prompt.status_parts(),
        Some(Modal::About(about)) => about.status_parts(),
    }
}
