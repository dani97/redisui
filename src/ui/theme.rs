use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Block;

use crate::state::DataType;

pub const BORDER: Color = Color::DarkGray;
pub const ACCENT: Color = Color::Cyan;
pub const DIM: Color = Color::Gray;
pub const TEXT: Color = Color::White;
pub const MUTED: Color = Color::DarkGray;
pub const ERROR: Color = Color::Red;
pub const OK: Color = Color::Green;

pub const FOCUSED_MARK: &str = "▸";
pub const UNFOCUSED_MARK: &str = " ";
pub const SPINNER: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];

impl DataType {
    pub fn color(self) -> Color {
        match self {
            DataType::String => Color::Green,
            DataType::List => Color::Cyan,
            DataType::Hash => Color::Yellow,
            DataType::Set => Color::Magenta,
            DataType::SortedSet => Color::Blue,
        }
    }
}

pub fn block(title: String, focused: bool) -> Block<'static> {
    let color = if focused { ACCENT } else { BORDER };
    Block::bordered()
        .border_style(Style::default().fg(color))
        .title(Line::from(title).style(Style::default().fg(color).bold()))
}

/// Pane titles carry a focus marker so the active pane is obvious at a glance.
pub fn pane_title(focused: bool, label: &str) -> String {
    let mark = if focused {
        FOCUSED_MARK
    } else {
        UNFOCUSED_MARK
    };
    format!("{mark} {label}")
}

pub fn selected() -> Style {
    Style::default()
        .bg(Color::Indexed(238))
        .fg(TEXT)
        .add_modifier(Modifier::BOLD)
}

pub fn dim() -> Style {
    Style::default().fg(DIM)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn spinner(tick: u64) -> &'static str {
    SPINNER[(tick as usize) % SPINNER.len()]
}
