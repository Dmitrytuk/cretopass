//! Applying the persisted settings while the Configuration dialog is open.
//!
//! The colour scheme is applied at once — the user sees it live — but nothing is
//! written to disk until the user saves.

use crate::theming::{Palette, set_palette, toggled_scheme};

use super::super::HomeState;
use super::super::modal::Setting;
/// Flips one setting. The colour scheme is applied at once, so the user sees it
/// while the dialog is still open; it is persisted only on save.
pub(super) fn apply_setting(state: &mut HomeState, setting: Setting) {
    match setting {
        Setting::DisplayFullPath => {
            state.config.display_full_path = !state.config.display_full_path
        }
        Setting::OpenLastFileOnStartup => {
            state.config.open_last_opened_file_on_startup =
                !state.config.open_last_opened_file_on_startup;
        }
        Setting::ColourScheme => {
            state.config.palette_name = toggled_scheme(&state.config.palette_name).to_string();
            set_palette(Palette::from_name(&state.config.palette_name));
        }
    }
}
