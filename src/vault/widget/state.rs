//! The Home screen's state: the menu cursor, the open vault, and the screen-wide
//! decorations a modal needs (busy status, spinner phase, browser memory).

use crate::file_browser::FileBrowserSession;
use crate::file_handling;
use crate::generator::PasswordParams;
use crate::theming::{Palette, Spinner, Status, set_palette, texts};
use crate::vault::active_file::ActiveVaultState;
use crate::vault::utils::{self, Configuration};

use super::modal::{Modal, UnlockPrompt};

pub struct HomeState {
    pub(super) selected: usize,
    pub config: Configuration,
    pub quit: bool,
    pub active_vault: Option<ActiveVaultState>,
    pub(super) modal: Option<Modal>,
    /// Title-bar status of a blocking operation, if any.
    pub status: Option<Status>,
    /// Frame counter for the animated busy glyph shown next to [`HomeState::status`].
    pub(super) spinner: Spinner,
    /// In-memory memory shared by successive file browser openings.
    pub(super) browser_session: FileBrowserSession,
    /// Password-generator settings for this app run; seeded into each opened
    /// tree and copied back when it exits. Never persisted to the config file.
    pub generator: PasswordParams,
}

impl Default for HomeState {
    fn default() -> Self {
        Self::new()
    }
}

impl HomeState {
    pub fn new() -> Self {
        let config = utils::load_config();
        // Apply the persisted colour scheme before the first frame is drawn.
        set_palette(Palette::from_name(&config.palette_name));
        let modal = if config.open_last_opened_file_on_startup {
            config.last_opened_file.as_ref().and_then(|path| {
                file_handling::validate_encrypted_file(path)
                    .ok()
                    .map(|data| Modal::Unlock(UnlockPrompt::new(path.clone(), data)))
            })
        } else {
            None
        };
        Self {
            selected: 0,
            config,
            quit: false,
            active_vault: None,
            modal,
            status: None,
            spinner: Spinner::default(),
            browser_session: FileBrowserSession::default(),
            generator: PasswordParams::default(),
        }
    }

    pub(super) fn total_navigable(&self) -> usize {
        texts::MENU_ITEMS.len() + self.config.recent_files.len()
    }

    pub(super) fn select_next(&mut self) {
        let total = self.total_navigable();
        if total == 0 {
            return;
        }
        self.selected = (self.selected + 1) % total;
    }

    pub(super) fn select_prev(&mut self) {
        let total = self.total_navigable();
        if total == 0 {
            return;
        }
        self.selected = (self.selected + total - 1) % total;
    }
}
