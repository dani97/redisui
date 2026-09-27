use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::{AppState, Mode};
use crate::ui::{text, theme};

const GAP: usize = 2;

pub fn status(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    if state.mode == Mode::Filter {
        render_filter(frame, area, state);
        return;
    }

    let left = left_spans(state);
    let right = right_spans(state);

    let right_width = (measure(&right) + GAP) as u16;
    let right_area = Rect {
        x: area.x + area.width.saturating_sub(right_width),
        width: right_width.min(area.width),
        ..area
    };
    let left_area = Rect {
        width: area.width.saturating_sub(right_width),
        ..area
    };

    let bar = Style::default().bg(theme::BORDER);
    frame.render_widget(Paragraph::new(Line::from(left)).style(bar), left_area);
    frame.render_widget(Paragraph::new(Line::from(right)).style(bar), right_area);
}

fn render_filter(frame: &mut Frame<'_>, area: Rect, state: &AppState) {
    let bar = Style::default().bg(theme::BORDER);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("/", Style::default().fg(theme::ACCENT).bold()),
            Span::styled(state.filter.clone(), Style::default().fg(theme::TEXT)),
            Span::styled("▏", Style::default().fg(theme::ACCENT)),
            Span::raw("  "),
            Span::styled("enter:apply  esc:cancel", theme::muted()),
        ]))
        .style(bar),
        area,
    );
}

fn left_spans(state: &AppState) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    if state.server.connected {
        spans.push(Span::styled("● connected", Style::default().fg(theme::OK)));
    } else if state.connecting {
        spans.push(Span::styled(
            format!("{} connecting…", theme::spinner(state.tick)),
            theme::dim(),
        ));
    } else {
        spans.push(Span::styled("● offline", Style::default().fg(theme::ERROR)));
    }

    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        state.session.display_url.clone(),
        Style::default().fg(theme::TEXT),
    ));

    if let Some(info) = &state.server.info {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("v{}", info.version), theme::dim()));
    }

    if state.scan.in_flight {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("{} scanning…", theme::spinner(state.tick)),
            theme::dim(),
        ));
    }

    spans
}

fn right_spans(state: &AppState) -> Vec<Span<'static>> {
    if let Some(status) = &state.status {
        let color = if status.is_error {
            theme::ERROR
        } else {
            theme::DIM
        };
        return vec![Span::styled(
            status.text.clone(),
            Style::default().fg(color),
        )];
    }

    vec![Span::styled(
        "tab:panes /:filter r:refresh ?:help q:quit",
        theme::muted(),
    )]
}

fn measure(spans: &[Span<'_>]) -> usize {
    spans
        .iter()
        .map(|span| text::width(span.content.as_ref()))
        .sum()
}
