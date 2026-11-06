//! The export flow's first step: choosing a format.
//!
//! The destination is named afterwards, in the File Browser. The format travels
//! as an [`ExportFormat`], so the browser never round-trips through its display
//! label.

use std::borrow::Cow;

use crossterm::event::{KeyEvent, MouseButton};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::HomeAction;
use crate::file_handling::ExportFormat;
use crate::hitbox::ClickSurface;
use crate::theming::{Action, ElementStyle, Icon, texts};
use crate::vault::widget::chrome::{make_overlay, paint_overlay};

/// The formats in the order [`texts::EXPORT_TYPE_ITEMS`] lists them.
const FORMATS: [ExportFormat; 3] = [ExportFormat::Json, ExportFormat::Xml, ExportFormat::Csv];

/// Choosing the format of an export.
pub struct ExportWizard {
    /// Focused row of the [`texts::EXPORT_TYPE_ITEMS`] list.
    index: usize,
    /// The clickable format rows and the panel the renderer drew, so a left click
    /// selects a format and a click outside the panel dismisses the wizard.
    clicks: ClickSurface<usize>,
}

impl ExportWizard {
    /// A wizard with the first format highlighted.
    pub fn new() -> Self {
        Self {
            index: 0,
            clicks: ClickSurface::default(),
        }
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

    /// A left click on a format row selects it, or on a repeat chooses it.
    pub fn handle_click(&mut self, button: MouseButton, x: u16, y: u16) -> Option<HomeAction> {
        let row = self.clicks.target_at(button, x, y)?;
        let count = FORMATS.len();
        if row >= count {
            return None;
        }
        let was_selected = self.index == row;
        self.index = row;
        if was_selected {
            Some(HomeAction::BeginExportDestination {
                format: FORMATS[row],
            })
        } else {
            None
        }
    }

    /// The panel the renderer drew last frame, or `None` before the first frame.
    /// A left click outside it dismisses the wizard, like `Esc`.
    pub(crate) fn panel_rect(&self) -> Option<Rect> {
        self.clicks.panel()
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        self.clicks.clear();
        let panel = render_format_list(frame, area, self.index, &mut self.clicks);
        self.clicks.set_panel(panel);
    }

    pub fn status_parts(&self) -> (&'static str, ElementStyle, Cow<'_, str>) {
        (
            texts::MODE_EXPORT,
            ElementStyle::ModeFieldType,
            Cow::Borrowed(texts::HINT_EXPORT_SELECT),
        )
    }
}

fn render_format_list(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    clicks: &mut ClickSurface<usize>,
) -> Rect {
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

    let inner = block.inner(overlay_area);
    let visible = texts::EXPORT_TYPE_ITEMS
        .len()
        .min(inner.height.saturating_sub(1) as usize);
    for i in 0..visible {
        let rect = Rect::new(inner.x, inner.y + 1 + i as u16, inner.width, 1);
        clicks.record(rect, i);
    }

    paint_overlay(
        frame,
        overlay_area,
        block,
        Paragraph::new(Text::from(lines)),
    );

    overlay_area
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
