use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

use crate::state::{AppState, Focus};
use crate::ui::{text, theme};

pub fn keys(frame: &mut Frame<'_>, area: Rect, state: &mut AppState) {
    let focused = state.focus == Focus::Keys;
    let data_type = state.selected_type();
    let bucket = state.scan.bucket(data_type);
    let count = bucket.map(|bucket| bucket.count).unwrap_or(0);
    let shown = bucket.map(|bucket| bucket.keys.len()).unwrap_or(0);

    let mut title = format!(
        "{}  {}  {shown}/{count}",
        theme::pane_title(focused, "keys"),
        data_type.label(),
    );
    if bucket.is_some_and(|bucket| bucket.truncated()) {
        title.push_str(" +");
    }
    if !state.scan.filter.is_empty() {
        title.push_str(&format!("  /{}", state.scan.filter));
    }

    let inner = area.width.saturating_sub(4) as usize;
    let names = bucket.map(|bucket| bucket.keys.as_slice()).unwrap_or(&[]);
    let items: Vec<ListItem> = names
        .iter()
        .map(|name| ListItem::new(Line::from(Span::raw(text::elide(name, inner)))))
        .collect();

    let list = List::new(items)
        .block(theme::block(title, focused))
        .highlight_style(theme::selected())
        .highlight_symbol("");

    frame.render_stateful_widget(list, area, &mut state.lists.keys);

    if names.is_empty() {
        let hint = if state.scan.in_flight {
            "scanning…"
        } else {
            "no keys"
        };
        frame.render_widget(
            Line::from(Span::styled(hint, theme::muted())),
            inner_centered(area),
        );
    }
}

fn inner_centered(area: Rect) -> Rect {
    Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: 1,
    }
}
