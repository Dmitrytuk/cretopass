//! The two-step export flow: pick a format, then confirm it.
//!
//! The format travels as an [`ExportFormat`] through both steps, so the choice
//! never round-trips through its display label.

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

/// One export run, from choosing a format to confirming it.
pub struct ExportWizard {
    step: Step,
}

/// How far the wizard has got.
#[derive(Clone, Copy)]
enum Step {
    /// Choosing one of the [`texts::EXPORT_TYPE_ITEMS`].
    SelectFormat { index: usize },
    /// Confirming the chosen format.
    Confirm { format: ExportFormat },
}

impl ExportWizard {
    /// A wizard at the first step, with the first format highlighted.
    pub fn new() -> Self {
        Self {
            step: Step::SelectFormat { index: 0 },
        }
    }

    /// `j`/`k` and `1`-`9` move through the formats, `Enter` advances to the
    /// confirmation, and the confirmation's `y`/`Enter` asks for the export.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<HomeAction> {
        match &self.step {
            Step::SelectFormat { .. } => self.select_format(key),
            Step::Confirm { .. } => self.confirm_format(key),
        }
    }

    /// Moves through the format list; `Enter` moves on to the confirmation.
    fn select_format(&mut self, key: KeyEvent) -> Option<HomeAction> {
        let Step::SelectFormat { index } = &mut self.step else {
            return None;
        };

        let count = FORMATS.len();
        match Action::from_select_field_type(&key) {
            Action::Cancel => Some(HomeAction::Close),
            Action::NavDown => {
                *index = (*index + 1) % count;
                None
            }
            Action::NavUp => {
                *index = (*index + count - 1) % count;
                None
            }
            Action::SelectIndex(i) if i < count => {
                *index = i;
                None
            }
            Action::Confirm => {
                self.step = Step::Confirm {
                    format: FORMATS[*index],
                };
                None
            }
            _ => None,
        }
    }

    /// `y`/`Enter` asks for the export; every other key declines.
    fn confirm_format(&mut self, key: KeyEvent) -> Option<HomeAction> {
        let Step::Confirm { format } = self.step else {
            return None;
        };

        match Action::from_confirm(&key) {
            Action::Yes => Some(HomeAction::Export { format }),
            _ => Some(HomeAction::Close),
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        match &self.step {
            Step::SelectFormat { index } => render_format_list(frame, area, *index),
            Step::Confirm { format } => render_confirmation(frame, area, *format),
        }
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        let hint = match &self.step {
            Step::SelectFormat { .. } => texts::HINT_EXPORT_SELECT,
            Step::Confirm { .. } => texts::HINT_EXPORT_CONFIRM,
        };
        (
            texts::MODE_EXPORT,
            ElementStyle::ModeFieldType,
            Cow::Borrowed(hint),
        )
    }
}

/// The human-readable name of `format`, as the export prompt shows it.
fn label(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Json => texts::FORMAT_JSON,
        ExportFormat::Xml => texts::FORMAT_XML,
        ExportFormat::Csv => texts::FORMAT_CSV,
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

fn render_confirmation(frame: &mut Frame, area: Rect, format: ExportFormat) {
    let overlay_area = make_overlay(area, 6);
    let block = Block::default()
        .title(texts::EXPORT_CONFIRM_TITLE)
        .borders(Borders::ALL)
        .style(ElementStyle::HomeChangePwBlock.style());

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!(
                "{}{}{}",
                texts::PROMPT_EXPORT_CONFIRM_START,
                label(format),
                texts::PROMPT_EXPORT_CONFIRM_END
            ),
            ElementStyle::ConfirmMessage.style(),
        )),
        Line::from(""),
    ];

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
    fn choosing_a_format_advances_to_the_confirmation() {
        let mut wizard = ExportWizard::new();
        assert!(wizard.handle_key(key(KeyCode::Char('j'))).is_none());
        assert!(wizard.handle_key(key(KeyCode::Enter)).is_none());
        assert!(matches!(
            wizard.step,
            Step::Confirm {
                format: ExportFormat::Xml
            }
        ));
    }

    #[test]
    fn number_keys_pick_a_format_directly() {
        let mut wizard = ExportWizard::new();
        assert!(wizard.handle_key(key(KeyCode::Char('3'))).is_none());
        wizard.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            wizard.step,
            Step::Confirm {
                format: ExportFormat::Csv
            }
        ));
    }

    #[test]
    fn confirming_asks_for_the_export() {
        let mut wizard = ExportWizard::new();
        wizard.handle_key(key(KeyCode::Enter));
        let action = wizard.handle_key(key(KeyCode::Char('y')));
        assert!(matches!(
            action,
            Some(HomeAction::Export {
                format: ExportFormat::Json
            })
        ));
    }

    #[test]
    fn declining_the_confirmation_closes_the_wizard() {
        let mut wizard = ExportWizard::new();
        wizard.handle_key(key(KeyCode::Enter));
        assert!(matches!(
            wizard.handle_key(key(KeyCode::Esc)),
            Some(HomeAction::Close)
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
}
