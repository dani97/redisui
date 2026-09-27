use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::ui::theme;

const ENTRIES: &[(&str, &str)] = &[
    ("tab / shift+tab", "next / previous pane"),
    ("↑ ↓  or  k j", "move selection"),
    ("page up / page down", "move by ten"),
    ("home / end  or  g G", "jump to first / last"),
    ("enter", "types → keys → value"),
    ("/", "filter keys by glob pattern"),
    ("r", "rescan the keyspace"),
    ("?", "toggle this help"),
    ("q  or  ctrl+c", "quit"),
];

pub fn overlay(frame: &mut Frame<'_>, area: Rect) {
    let width = 56.min(area.width.saturating_sub(4));
    let height = (ENTRIES.len() as u16 + 4).min(area.height.saturating_sub(2));
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };

    let block = Block::bordered()
        .border_style(Style::default().fg(theme::ACCENT))
        .title(Line::from(" keys ").style(Style::default().fg(theme::ACCENT).bold()));

    let mut lines: Vec<Line> = ENTRIES
        .iter()
        .map(|(key, description)| {
            Line::from(vec![
                Span::styled(format!("{key:<20}"), Style::default().fg(theme::ACCENT)),
                Span::styled(*description, theme::dim()),
            ])
        })
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "any key closes this",
        theme::muted().italic(),
    )));

    frame.render_widget(Clear, popup);
    frame.render_widget(Paragraph::new(lines).block(block), popup);
}
