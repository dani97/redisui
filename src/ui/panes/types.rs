use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

use crate::state::{AppState, DataType, Focus};
use crate::ui::{text, theme};

const COUNT_WIDTH: usize = 10;

pub fn types(frame: &mut Frame<'_>, area: Rect, state: &mut AppState) {
    let focused = state.focus == Focus::Types;
    let title = theme::pane_title(focused, "types");

    let items: Vec<ListItem> = DataType::ALL
        .iter()
        .map(|data_type| {
            let count = state.scan.bucket(*data_type).map(|b| b.count).unwrap_or(0);
            let label = text::pad(data_type.label(), DataType::WIDTH);
            let count = text::pad(&count.to_string(), COUNT_WIDTH);
            ListItem::new(Line::from(vec![
                Span::styled(label, Style::default().fg(data_type.color()).bold()),
                Span::styled(count, theme::dim()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(theme::block(title, focused))
        .highlight_style(theme::selected())
        .highlight_symbol("");

    frame.render_stateful_widget(list, area, &mut state.lists.types);
}
