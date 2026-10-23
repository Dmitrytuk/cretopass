//! The Configuration modal: the persisted Home preferences.
//!
//! Editing is a pure request: the modal reports which setting the user toggled
//! and lets the orchestrator apply and persist it, because applying a setting —
//! not least swapping the colour scheme — is a screen-wide side effect.

use std::borrow::Cow;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::theming::{Action, ElementStyle, Icon, scheme_label, texts};
use crate::vault::utils::Configuration;
use crate::vault::widget::HomeAction;
use crate::vault::widget::chrome::{make_overlay, paint_overlay};

/// A row of the Configuration modal, in the order it is listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    /// Show whole paths instead of bare file names.
    DisplayFullPath,
    /// Reopen the last secret file on the next start.
    OpenLastFileOnStartup,
    /// Cycle between the available colour schemes.
    ColourScheme,
}

const SETTINGS: [Setting; 3] = [
    Setting::DisplayFullPath,
    Setting::OpenLastFileOnStartup,
    Setting::ColourScheme,
];

/// The Home configuration dialog.
pub struct ConfigurationModal {
    selected: usize,
}

impl ConfigurationModal {
    /// A dialog with the first setting highlighted.
    pub fn new() -> Self {
        Self { selected: 0 }
    }

    /// `j`/`k` move between settings, `Space` toggles the highlighted one,
    /// `Enter` persists the configuration, and `Esc` discards the changes.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match Action::from_config(&key) {
            Action::NavDown => {
                self.selected = (self.selected + 1).min(SETTINGS.len() - 1);
                None
            }
            Action::NavUp => {
                self.selected = self.selected.saturating_sub(1);
                None
            }
            Action::ToggleSetting => Some(HomeAction::ToggleSetting(SETTINGS[self.selected])),
            Action::SaveConfig => Some(HomeAction::SaveConfig),
            Action::Cancel => Some(HomeAction::DiscardConfig),
            _ => None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, config: &Configuration) {
        let overlay_area = make_overlay(area, 10);
        let block = Block::default()
            .title(texts::CONFIG_TITLE)
            .borders(Borders::ALL)
            .style(ElementStyle::HomeOverlayBlock.style());

        let mut lines: Vec<Line> = Vec::new();
        lines.push(Line::from(""));

        for (i, setting) in SETTINGS.iter().enumerate() {
            let selected = i == self.selected;
            let style = if selected {
                ElementStyle::HomeSelected.style()
            } else {
                ElementStyle::HomeNormal.style()
            };
            let prefix = if selected {
                Icon::SelectionArrow.as_str()
            } else {
                Icon::SelectionPad.as_str()
            };
            lines.push(Line::from(row(setting, prefix, style, config)));
        }

        lines.push(Line::from(""));

        paint_overlay(
            frame,
            overlay_area,
            block,
            Paragraph::new(Text::from(lines)),
        );
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        (
            texts::MODE_CONFIG,
            ElementStyle::ModeFieldType,
            Cow::Borrowed(texts::HINT_CONFIG),
        )
    }
}

/// One rendered setting row: two checkboxes, and a cyclic value for the scheme.
fn row(
    setting: &Setting,
    prefix: &str,
    style: ratatui::style::Style,
    config: &Configuration,
) -> Vec<Span<'static>> {
    match setting {
        Setting::DisplayFullPath => vec![
            Span::styled(
                format!("{}{} ", prefix, checkbox(config.display_full_path)),
                style,
            ),
            Span::styled(texts::SETTING_FULL_PATH, style),
        ],
        Setting::OpenLastFileOnStartup => vec![
            Span::styled(
                format!(
                    "{}{} ",
                    prefix,
                    checkbox(config.open_last_opened_file_on_startup)
                ),
                style,
            ),
            Span::styled(texts::SETTING_LAST_FILE, style),
        ],
        // The colour scheme is a cyclic choice, not an on/off flag, so it shows
        // its current value instead of a checkbox.
        Setting::ColourScheme => vec![Span::styled(
            format!(
                "{}{}: {}",
                prefix,
                texts::SETTING_PALETTE,
                scheme_label(&config.palette_name)
            ),
            style,
        )],
    }
}

fn checkbox(value: bool) -> &'static str {
    if value {
        Icon::CheckboxOn.as_str()
    } else {
        Icon::CheckboxOff.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    #[test]
    fn space_asks_to_toggle_the_highlighted_setting() {
        let mut modal = ConfigurationModal::new();
        assert!(matches!(
            modal.handle_key(key(KeyCode::Char(' '))),
            Some(HomeAction::ToggleSetting(Setting::DisplayFullPath))
        ));

        modal.handle_key(key(KeyCode::Char('j')));
        modal.handle_key(key(KeyCode::Char('j')));
        assert!(matches!(
            modal.handle_key(key(KeyCode::Char(' '))),
            Some(HomeAction::ToggleSetting(Setting::ColourScheme))
        ));
    }

    #[test]
    fn navigation_stops_at_the_last_setting() {
        let mut modal = ConfigurationModal::new();
        for _ in 0..10 {
            modal.handle_key(key(KeyCode::Char('j')));
        }
        assert!(matches!(
            modal.handle_key(key(KeyCode::Char(' '))),
            Some(HomeAction::ToggleSetting(Setting::ColourScheme))
        ));
    }

    #[test]
    fn enter_persists_and_esc_discards() {
        let mut modal = ConfigurationModal::new();
        assert!(matches!(
            modal.handle_key(key(KeyCode::Enter)),
            Some(HomeAction::SaveConfig)
        ));
        assert!(matches!(
            modal.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::DiscardConfig)
        ));
    }
}
