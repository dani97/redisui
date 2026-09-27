use ratatui::Frame;

use crate::state::AppState;

pub mod help;
pub mod layout;
pub mod panes;
pub mod text;
pub mod theme;

pub fn draw(frame: &mut Frame<'_>, state: &mut AppState) {
    let rects = layout::panes(frame.area());

    panes::types_pane(frame, rects.types, state);
    panes::keys_pane(frame, rects.keys, state);
    panes::value_pane(frame, rects.value, state);
    panes::detail_pane(frame, rects.detail, state);
    panes::status_pane(frame, rects.status, state);

    if state.show_help {
        help::overlay(frame, frame.area());
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::cli::Args;
    use crate::state::{DataType, KeyDetail, Ttl, ValueView};
    use crate::store::Session;

    fn state() -> AppState {
        let session = Session::new(&Args::parse_from([
            "redisui",
            "--url",
            "redis://127.0.0.1:6379",
        ]))
        .expect("valid url");
        AppState::new(session)
    }

    fn render(width: u16, height: u16, state: &mut AppState) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| draw(frame, state)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let area = *buffer.area();
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn renders_every_pane_while_offline() {
        let mut state = state();
        let screen = render(100, 30, &mut state);

        assert!(screen.contains("types"));
        assert!(screen.contains("strings"));
        assert!(screen.contains("sorted sets"));
        assert!(screen.contains("keys"));
        assert!(screen.contains("value"));
        assert!(screen.contains("detail"));
        assert!(screen.contains("offline"));
    }

    #[test]
    fn shows_keys_and_counts_for_the_selected_type() {
        let mut state = state();
        state.scan.buckets.insert(
            DataType::String,
            crate::state::TypeBucket {
                count: 2,
                keys: vec!["user:1".to_string(), "user:2".to_string()],
            },
        );

        let screen = render(100, 30, &mut state);

        assert!(screen.contains("user:1"));
        assert!(screen.contains("user:2"));
        assert!(screen.contains("2/2"));
    }

    #[test]
    fn renders_hash_rows_under_a_column_header() {
        let mut state = state();
        state.value.id = 1;
        state.value.key = "user:1".to_string();
        state.value.view = Some(ValueView::Rows {
            headers: vec!["field".to_string(), "value".to_string()],
            rows: vec![
                vec!["name".to_string(), "alice".to_string()],
                vec!["email".to_string(), "a@b.c".to_string()],
            ],
            total: 2,
        });

        let screen = render(100, 30, &mut state);

        assert!(screen.contains("field"));
        assert!(screen.contains("name"));
        assert!(screen.contains("alice"));
        assert!(screen.contains("2 rows"));
    }

    #[test]
    fn renders_key_detail_fields() {
        let mut state = state();
        state.detail.id = 1;
        state.detail.key = "user:1".to_string();
        state.detail.detail = Some(KeyDetail {
            data_type: DataType::Hash,
            ttl: Ttl::Expiring(120),
            encoding: "listpack".to_string(),
            memory_bytes: Some(128),
        });

        let screen = render(100, 30, &mut state);

        assert!(screen.contains("encoding"));
        assert!(screen.contains("listpack"));
        assert!(screen.contains("120s"));
        assert!(screen.contains("128 B"));
    }

    #[test]
    fn filter_mode_replaces_the_status_hints() {
        let mut state = state();
        state.mode = crate::state::Mode::Filter;
        state.filter = "user:*".to_string();

        let screen = render(100, 30, &mut state);

        assert!(screen.contains("/user:*"));
        assert!(screen.contains("enter:apply"));
    }

    #[test]
    fn help_overlay_covers_the_panes() {
        let mut state = state();
        state.show_help = true;

        let screen = render(100, 30, &mut state);

        assert!(screen.contains("next / previous pane"));
        assert!(screen.contains("quit"));
    }

    #[test]
    fn survives_a_tiny_terminal() {
        let mut state = state();
        for (width, height) in [(1, 1), (4, 3), (20, 6), (200, 60)] {
            render(width, height, &mut state);
        }
    }
}
