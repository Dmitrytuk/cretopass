//! Key handling for the Home menu.
//!
//! The menu is the screen itself, not a modal: it reports what the user asked for
//! as a [`HomeAction`] and the orchestrator performs it,
//! exactly as the modals do.

use std::io;

use crossterm::event;
use ratatui::Terminal;
use ratatui::backend::Backend;

use crate::theming::{Action, texts};

use super::modal::HomeAction;
use super::orchestrator;
use super::state::HomeState;

// ── Settings menu key handling (no modal active) ──// ── Settings menu key handling (no modal active) ──

/// Handles a key on the Home menu.
///
/// Moving the cursor is pure screen state and handled here; everything else —
/// quitting, opening a modal, launching the Tree, touching the recent list — is
/// reported as a [`HomeAction`] and performed by the orchestrator, exactly as
/// the modals do.
pub fn handle_home_key<B: Backend>(
    key: event::KeyEvent,
    state: &mut HomeState,
    terminal: &mut Terminal<B>,
) -> io::Result<()> {
    let action = Action::from_home(&key);
    match &action {
        Action::NavDown => state.select_next(),
        Action::NavUp => state.select_prev(),
        _ => {
            if let Some(intent) = menu_intent(state, &action) {
                return orchestrator::execute(state, terminal, intent, None);
            }
        }
    }
    Ok(())
}

/// What the Home menu asks for, given the highlighted row.
///
/// `None` means the key needs nothing from the orchestrator: it either moved the
/// cursor or is not bound in this menu.
fn menu_intent(state: &HomeState, action: &Action) -> Option<HomeAction> {
    match action {
        Action::Quit => Some(HomeAction::Quit),
        Action::ReturnToMenu => Some(HomeAction::LaunchTree),
        Action::Back => Some(HomeAction::Back),
        Action::OpenHelp => Some(HomeAction::OpenAbout),
        Action::Confirm => match state.selected {
            0 => Some(HomeAction::ChooseFileToOpen),
            1 => Some(HomeAction::ChooseFileToCreate),
            2 => Some(HomeAction::OpenConfiguration),
            3 => Some(HomeAction::BeginChangePassword),
            4 => Some(HomeAction::BeginExport),
            5 => Some(HomeAction::ChooseFileToImport),
            idx => state
                .config
                .recent_files
                .get(idx - texts::MENU_ITEMS.len())
                .map(|path| HomeAction::OpenRecent { path: path.clone() }),
        },
        Action::ConfigDelete => state
            .selected
            .checked_sub(texts::MENU_ITEMS.len())
            .and_then(|idx| state.config.recent_files.get(idx))
            .map(|path| HomeAction::RequestDeleteRecent { path: path.clone() }),
        _ => None,
    }
}
