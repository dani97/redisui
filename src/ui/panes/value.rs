use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem};

use crate::state::{AppState, Focus, ValueView};
use crate::ui::{text, theme};

pub fn value(frame: &mut Frame<'_>, area: Rect, state: &mut AppState) {
    let focused = state.focus == Focus::Value;
    let block = theme::block(value_title(state), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let columns: Option<(Vec<String>, Vec<Vec<String>>)> = match &state.value.view {
        Some(ValueView::Rows { headers, rows, .. }) => Some((headers.clone(), rows.clone())),
        _ => None,
    };

    let [header_area, list_area] = if columns.is_some() {
        Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner)
    } else {
        [Rect::ZERO, inner]
    };

    let budget = list_area.width as usize;
    let items: Vec<ListItem> = match &columns {
        Some((headers, rows)) => {
            let widths = text::column_widths(headers, rows);
            frame.render_widget(header_spans(headers, &widths), header_area);
            rows.iter()
                .map(|row| ListItem::new(row_spans(row, &widths, budget)))
                .collect()
        }
        None => match &state.value.view {
            Some(ValueView::Scalar { text: body, .. }) => body
                .lines()
                .map(|line| ListItem::new(Line::from(Span::raw(text::elide(line, budget)))))
                .collect(),
            _ => Vec::new(),
        },
    };

    let list = List::new(items)
        .highlight_style(theme::selected())
        .highlight_symbol("");
    frame.render_stateful_widget(list, list_area, &mut state.lists.value);

    if state.value.key.is_empty() {
        hint(frame, list_area, "no key selected");
    } else if state.value.loading {
        hint(
            frame,
            list_area,
            &format!("loading {}…", theme::spinner(state.tick)),
        );
    }
}

fn hint(frame: &mut Frame<'_>, area: Rect, message: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    frame.render_widget(
        Line::from(Span::styled(message.to_string(), theme::muted())),
        Rect {
            x: area.x + 1,
            y: area.y,
            width: area.width.saturating_sub(1),
            height: 1,
        },
    );
}

fn value_title(state: &AppState) -> String {
    let focused = state.focus == Focus::Value;
    let label = theme::pane_title(focused, "value");

    if state.value.key.is_empty() {
        return label;
    }

    let summary = match &state.value.view {
        Some(ValueView::Scalar { bytes, binary, .. }) => {
            let suffix = if *binary { ", binary" } else { "" };
            format!("{bytes} bytes{suffix}")
        }
        Some(ValueView::Rows { rows, total, .. }) => {
            if *total > rows.len() {
                format!("showing {} of {total} rows", rows.len())
            } else {
                format!("{} rows", rows.len())
            }
        }
        None => "loading…".to_string(),
    };

    format!("{label}  {}  {summary}", state.value.key)
}

fn header_spans(headers: &[String], widths: &[usize]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, header) in headers.iter().enumerate() {
        if index + 1 == headers.len() {
            spans.push(Span::styled(header.clone(), theme::muted().bold()));
        } else {
            spans.push(Span::styled(
                text::pad(header, widths.get(index).copied().unwrap_or(0)),
                theme::muted().bold(),
            ));
            spans.push(Span::raw(" "));
        }
    }
    Line::from(spans)
}

fn row_spans(row: &[String], widths: &[usize], budget: usize) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, cell) in row.iter().enumerate() {
        if index + 1 == row.len() {
            spans.push(Span::raw(text::elide(cell, budget)));
        } else {
            spans.push(Span::raw(text::pad(
                cell,
                widths.get(index).copied().unwrap_or(0),
            )));
            spans.push(Span::raw(" "));
        }
    }
    Line::from(spans)
}
