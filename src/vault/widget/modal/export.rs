//! The export flow's first step: choosing a format.
//!
//! The destination is named afterwards, in the File Browser. The format travels
//! as an [`ExportFormat`], so the browser never round-trips through its display
//! label.

use std::borrow::Cow;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::HomeAction;
use crate::file_handling::export::ExportFormat;
use crate::theming::{Action, ElementStyle, Icon, texts};
use crate::vault::widget::chrome::{make_overlay, paint_overlay};

/// The formats in the order [`texts::EXPORT_TYPE_ITEMS`] lists them.
const FORMATS: [ExportFormat; 3] = [ExportFormat::Json, ExportFormat::Xml, ExportFormat::Csv];

/// Choosing the format of an export.
pub struct ExportWizard {
    /// Focused row of the [`texts::EXPORT_TYPE_ITEMS`] list.
    index: usize,
}

impl ExportWizard {
    /// A wizard with the first format highlighted.
    pub fn new() -> Self {
        Self { index: 0 }
    }

    /// `j`/`k` and `1`-`9` move through the formats, and `Enter` asks for the
    /// destination of the chosen one.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        let count = FORMATS.len();
        match Action::from_select_field_type(&key) {
            Action::Cancel => Some(HomeAction::Close),
            Action::NavDown => {
                self.index = (self.index + 1) % count;
                None
            }
            Action::NavUp => {
                self.index = (self.index + count - 1) % count;
                None
            }
            Action::SelectIndex(i) if i < count => {
                self.index = i;
                None
            }
            Action::Confirm => Some(HomeAction::BeginExportDestination {
                format: FORMATS[self.index],
            }),
            _ => None,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        render_format_list(frame, area, self.index);
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        (
            texts::MODE_EXPORT,
            ElementStyle::ModeFieldType,
            Cow::Borrowed(texts::HINT_EXPORT_SELECT),
        )
    }
}

fn render_format_list(frame: &mut Frame, area: Rect, selected: usize) {
    let overlay_area = make_overlay(area, 9);
    let block = Block::default()
        .title(texts::EXPORT_SELECT_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeOverlayBlock.style());

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));

    for (i, item) in texts::EXPORT_TYPE_ITEMS.iter().enumerate() {
        let is_selected = i == selected;
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

    paint_overlay(
        frame,
        overlay_area,
        block,
        Paragraph::new(Text::from(lines)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyCode;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::from(code)
    }

    #[test]
    fn confirming_a_format_asks_for_the_destination() {
        let mut wizard = ExportWizard::new();
        wizard.handle_key(key(KeyCode::Char('j'))); // XML

        let action = wizard.handle_key(key(KeyCode::Enter));

        assert!(matches!(
            action,
            Some(HomeAction::BeginExportDestination {
                format: ExportFormat::Xml
            })
        ));
    }

    #[test]
    fn number_keys_pick_a_format_directly() {
        let mut wizard = ExportWizard::new();
        assert!(wizard.handle_key(key(KeyCode::Char('3'))).is_none());

        let action = wizard.handle_key(key(KeyCode::Enter));

        assert!(matches!(
            action,
            Some(HomeAction::BeginExportDestination {
                format: ExportFormat::Csv
            })
        ));
    }

    #[test]
    fn navigation_wraps_through_the_formats() {
        let mut wizard = ExportWizard::new();
        wizard.handle_key(key(KeyCode::Char('k'))); // wraps to CSV

        let action = wizard.handle_key(key(KeyCode::Enter));

        assert!(matches!(
            action,
            Some(HomeAction::BeginExportDestination {
                format: ExportFormat::Csv
            })
        ));
    }

    #[test]
    fn cancelling_the_format_list_closes_the_wizard() {
        let mut wizard = ExportWizard::new();
        assert!(matches!(
            wizard.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::Close)
        ));
    }

    #[test]
    fn status_parts_advertise_the_format_step() {
        let wizard = ExportWizard::new();

        let (mode, _, hint) = wizard.status_parts();

        assert_eq!(mode, texts::MODE_EXPORT);
        assert_eq!(hint, texts::HINT_EXPORT_SELECT);
    }
}
