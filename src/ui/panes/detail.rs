use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::{AppState, Focus};
use crate::ui::{text, theme};

const LABEL_WIDTH: usize = 10;

pub fn detail(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let focused = state.focus == Focus::Detail;

    let title = if state.detail.key.is_empty() {
        theme::pane_title(focused, "detail")
    } else {
        format!(
            "{}  {}",
            theme::pane_title(focused, "detail"),
            state.detail.key
        )
    };

    let block = theme::block(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let lines: Vec<Line> = if state.detail.loading {
        vec![Line::from(Span::styled(
            format!("loading {}…", theme::spinner(state.tick)),
            theme::muted(),
        ))]
    } else {
        match &state.detail.detail {
            Some(detail) => vec![
                field("type", detail.data_type.label().to_string()),
                field("ttl", detail.ttl.label()),
                field("encoding", detail.encoding.clone()),
                field(
                    "memory",
                    match detail.memory_bytes {
                        Some(bytes) => format!("{bytes} B"),
                        None => "n/a".to_string(),
                    },
                ),
            ],
            None => vec![Line::from(Span::styled(
                if state.detail.key.is_empty() {
                    "no key selected"
                } else {
                    "no details"
                },
                theme::muted(),
            ))],
        }
    };

    frame.render_widget(Paragraph::new(lines), inner);
}

fn field(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(text::pad(label, LABEL_WIDTH), theme::muted().bold()),
        Span::raw(text::elide(&value, 40)),
    ])
}
