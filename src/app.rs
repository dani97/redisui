use ratatui::widgets::ListState;
use tokio::sync::mpsc::unbounded_channel;

use crate::command::Command;
use crate::event::{Action, AppEvent, Events, dispatch};
use crate::state::{
    AppState, DataType, DetailState, FILTERED_SCAN_COUNT, Focus, HEARTBEAT_TICKS,
    KEYS_PER_TYPE_LIMIT, Mode, RECONNECT_TICKS, SCAN_COUNT, ScanState, ValueState,
};
use crate::store::{ScanPage, Session};
use crate::ui;

const PAGE: i32 = 10;

pub async fn run(terminal: &mut ratatui::DefaultTerminal, session: Session) -> anyhow::Result<()> {
    let (tx, rx) = unbounded_channel();
    let mut events = Events::new(rx);
    let mut state = AppState::new(session);

    for command in connect(&mut state) {
        dispatch(command, state.session.clone(), tx.clone());
    }

    while !state.should_quit {
        terminal.draw(|frame| ui::draw(frame, &mut state))?;
        let event = events.next().await;
        for command in reduce(&mut state, event) {
            dispatch(command, state.session.clone(), tx.clone());
        }
    }

    Ok(())
}

pub fn reduce(state: &mut AppState, event: AppEvent) -> Vec<Command> {
    match event {
        AppEvent::Key(key) => {
            let action = match state.mode {
                Mode::Filter => crate::event::translate_filter_key(key),
                Mode::Normal => key.into(),
            };
            on_action(state, action)
        }
        AppEvent::Action(action) => on_action(state, action),
        AppEvent::Resize => vec![],
        AppEvent::Tick => on_tick(state),
        AppEvent::InputError(error) => {
            state.set_status(format!("input error: {error}"), true);
            vec![]
        }
        AppEvent::Connected { conn, info } => {
            state.conn = Some(conn);
            state.connecting = false;
            state.server.connected = true;
            state.server.last_error = None;
            state.server.info = Some(info);
            state.set_status("connected", false);
            rescan(state)
        }
        AppEvent::ConnectFailed { error } => {
            state.connecting = false;
            state.server.connected = false;
            state.server.last_error = Some(error.clone());
            state.set_status(format!("connect failed: {error}"), true);
            vec![]
        }
        AppEvent::ScanProgress { id, page } => on_scan_progress(state, id, page),
        AppEvent::ScanFailed { id, error } => {
            state.scan.in_flight = false;
            if id == state.scan.id {
                state.set_status(format!("scan failed: {error}"), true);
            }
            vec![]
        }
        AppEvent::ValueLoaded { id, view } => {
            if id == state.value.id {
                state.value.view = Some(view);
                state.value.loading = false;
            }
            vec![]
        }
        AppEvent::ValueFailed { id, error } => {
            if id == state.value.id {
                state.value.loading = false;
                state.set_status(format!("value failed: {error}"), true);
            }
            vec![]
        }
        AppEvent::DetailLoaded { id, detail } => {
            if id == state.detail.id {
                state.detail.detail = Some(detail);
                state.detail.loading = false;
            }
            vec![]
        }
        AppEvent::DetailFailed { id, error } => {
            if id == state.detail.id {
                state.detail.loading = false;
                state.set_status(format!("detail failed: {error}"), true);
            }
            vec![]
        }
        AppEvent::HeartbeatOk => {
            state.heartbeat_in_flight = false;
            if state.server.connected {
                return vec![];
            }
            state.server.connected = true;
            state.server.last_error = None;
            state.set_status("reconnected", false);
            rescan(state)
        }
        AppEvent::HeartbeatFailed { error } => {
            state.heartbeat_in_flight = false;
            state.server.connected = false;
            state.server.last_error = Some(error.clone());
            state.conn = None;
            state.set_status(format!("connection lost: {error}"), true);
            vec![]
        }
    }
}

fn on_action(state: &mut AppState, action: Action) -> Vec<Command> {
    if state.show_help {
        state.show_help = false;
        return vec![];
    }

    if state.mode == Mode::Filter {
        return on_filter_action(state, action);
    }

    match action {
        Action::Quit => {
            state.should_quit = true;
            vec![]
        }
        Action::Cancel => {
            state.show_help = false;
            vec![]
        }
        Action::ToggleHelp => {
            state.show_help = !state.show_help;
            vec![]
        }
        Action::NextPane | Action::PrevPane => {
            state.focus = if action == Action::NextPane {
                state.focus.next()
            } else {
                state.focus.prev()
            };
            load_selected(state)
        }
        Action::Up => move_cursor(state, -1),
        Action::Down => move_cursor(state, 1),
        Action::PageUp => move_cursor(state, -PAGE),
        Action::PageDown => move_cursor(state, PAGE),
        Action::Top => jump_to_end(state, false),
        Action::Bottom => jump_to_end(state, true),
        Action::Activate => {
            state.focus = match state.focus {
                Focus::Types => Focus::Keys,
                Focus::Keys => Focus::Value,
                other => other,
            };
            vec![]
        }
        Action::BeginFilter => {
            state.mode = Mode::Filter;
            vec![]
        }
        Action::Refresh => rescan(state),
        _ => vec![],
    }
}

fn on_filter_action(state: &mut AppState, action: Action) -> Vec<Command> {
    match action {
        Action::Char(c) => {
            state.filter.push(c);
            vec![]
        }
        Action::Backspace => {
            state.filter.pop();
            vec![]
        }
        Action::Activate => {
            state.mode = Mode::Normal;
            rescan(state)
        }
        Action::Cancel => {
            state.mode = Mode::Normal;
            state.filter = state.scan.filter.clone();
            vec![]
        }
        Action::Quit => {
            state.should_quit = true;
            vec![]
        }
        _ => vec![],
    }
}

fn on_tick(state: &mut AppState) -> Vec<Command> {
    state.tick = state.tick.wrapping_add(1);

    let Some(conn) = state.conn.clone() else {
        if state.connecting || !state.tick.is_multiple_of(RECONNECT_TICKS) {
            return vec![];
        }
        return connect(state);
    };

    if !heartbeat_due(state) {
        return vec![];
    }
    vec![Command::Heartbeat { conn }]
}

/// Liveness probes are due every `HEARTBEAT_TICKS` ticks, and only one may be
/// in flight at a time.
fn heartbeat_due(state: &mut AppState) -> bool {
    if state.heartbeat_in_flight || !state.tick.is_multiple_of(HEARTBEAT_TICKS) {
        return false;
    }
    state.heartbeat_in_flight = true;
    true
}

fn connect(state: &mut AppState) -> Vec<Command> {
    state.connecting = true;
    vec![Command::Connect]
}

fn on_scan_progress(state: &mut AppState, id: u64, page: ScanPage) -> Vec<Command> {
    if id != state.scan.id {
        return vec![];
    }

    state.scan.in_flight = false;
    state.scan.cursor = page.cursor;
    state.scan.done = page.done;
    state.scan.total_keys = page.total_keys;

    for (data_type, key) in page.keys {
        let bucket = state.scan.buckets.entry(data_type).or_default();
        bucket.count += 1;
        if bucket.keys.len() < KEYS_PER_TYPE_LIMIT {
            bucket.keys.push(key);
        }
    }

    clamp_keys(state);

    if page.done {
        let indexed: u64 = state
            .scan
            .buckets
            .values()
            .map(|bucket| bucket.count as u64)
            .sum();
        let total = state.scan.total_keys;
        if indexed == total && state.scan.filter.is_empty() {
            state.status = None;
        } else if state.scan.filter.is_empty() {
            state.set_status(format!("{indexed} of {total} keys indexed"), false);
        } else {
            state.set_status(format!("{indexed} of {total} match"), false);
        }
        return load_selected(state);
    }

    let Some(conn) = state.conn.clone() else {
        return vec![];
    };
    let filter = state.scan.filter.clone();
    let count = scan_count(&filter);
    state.scan.in_flight = true;
    vec![Command::Scan {
        id,
        conn,
        cursor: page.cursor,
        filter,
        count,
    }]
}

fn rescan(state: &mut AppState) -> Vec<Command> {
    let Some(conn) = state.conn.clone() else {
        return vec![];
    };

    let id = state.fresh_id();
    let filter = state.filter.clone();
    let count = scan_count(&filter);
    state.scan = ScanState {
        id,
        filter: filter.clone(),
        in_flight: true,
        ..Default::default()
    };
    state.lists.keys.select(Some(0));
    state.value = ValueState::default();
    state.detail = DetailState::default();

    vec![Command::Scan {
        id,
        conn,
        cursor: 0,
        filter,
        count,
    }]
}

fn load_selected(state: &mut AppState) -> Vec<Command> {
    let Some(conn) = state.conn.clone() else {
        return vec![];
    };
    let Some(key) = state.selected_key() else {
        return vec![];
    };

    let data_type = state.selected_type();
    let value_id = state.fresh_id();
    state.value.begin(value_id, key.clone());
    state.lists.value.select(Some(0));

    let detail_id = state.fresh_id();
    state.detail.begin(detail_id, key.clone());

    vec![
        Command::Value {
            id: value_id,
            conn: conn.clone(),
            key: key.clone(),
            data_type,
        },
        Command::Detail {
            id: detail_id,
            conn,
            key,
        },
    ]
}

fn move_cursor(state: &mut AppState, delta: i32) -> Vec<Command> {
    match state.focus {
        Focus::Types => {
            let len = DataType::ALL.len() as i32;
            if shift(&mut state.lists.types, len, delta) {
                state.lists.keys.select(Some(0));
                return load_selected(state);
            }
            vec![]
        }
        Focus::Keys => {
            let len = state.visible_keys().len() as i32;
            if shift(&mut state.lists.keys, len, delta) {
                return load_selected(state);
            }
            vec![]
        }
        Focus::Value => {
            let len = state.value.row_count() as i32;
            shift(&mut state.lists.value, len, delta);
            vec![]
        }
        Focus::Detail => vec![],
    }
}

fn jump_to_end(state: &mut AppState, to_end: bool) -> Vec<Command> {
    match state.focus {
        Focus::Types => {
            let target = if to_end { DataType::ALL.len() - 1 } else { 0 };
            if state.lists.types.selected() != Some(target) {
                state.lists.types.select(Some(target));
                state.lists.keys.select(Some(0));
                return load_selected(state);
            }
            vec![]
        }
        Focus::Keys => {
            let len = state.visible_keys().len();
            if len == 0 {
                return vec![];
            }
            let target = if to_end { len - 1 } else { 0 };
            if state.lists.keys.selected() != Some(target) {
                state.lists.keys.select(Some(target));
                return load_selected(state);
            }
            vec![]
        }
        Focus::Value => {
            let target = if to_end {
                state.value.row_count().saturating_sub(1)
            } else {
                0
            };
            state.lists.value.select(Some(target));
            vec![]
        }
        Focus::Detail => vec![],
    }
}

fn shift(list: &mut ListState, len: i32, delta: i32) -> bool {
    if len == 0 {
        return false;
    }
    let current = list.selected().unwrap_or(0) as i32;
    let next = current.saturating_add(delta).clamp(0, len - 1) as usize;
    if list.selected() == Some(next) {
        return false;
    }
    list.select(Some(next));
    true
}

fn clamp_keys(state: &mut AppState) {
    let len = state.visible_keys().len();
    if len == 0 {
        state.lists.keys.select(None);
        return;
    }
    let current = state.lists.keys.selected().unwrap_or(0);
    if current >= len {
        state.lists.keys.select(Some(len - 1));
    }
}

fn scan_count(filter: &str) -> usize {
    if filter.is_empty() {
        SCAN_COUNT
    } else {
        FILTERED_SCAN_COUNT
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::cli::Args;
    use crate::state::DataType;

    fn state() -> AppState {
        let session = Session::new(&Args::parse_from([
            "redisui",
            "--url",
            "redis://127.0.0.1:6379",
        ]))
        .expect("valid url");
        AppState::new(session)
    }

    fn page(cursor: u64, done: bool, keys: Vec<(DataType, String)>) -> AppEvent {
        AppEvent::ScanProgress {
            id: 0,
            page: ScanPage {
                cursor,
                total_keys: keys.len() as u64,
                done,
                keys,
            },
        }
    }

    #[test]
    fn quit_action_stops_the_loop() {
        let mut state = state();
        reduce(&mut state, AppEvent::Action(Action::Quit));
        assert!(state.should_quit);
    }

    #[test]
    fn tab_and_shift_tab_walk_the_panes() {
        let mut state = state();
        assert_eq!(state.focus, Focus::Types);

        reduce(&mut state, AppEvent::Action(Action::NextPane));
        assert_eq!(state.focus, Focus::Keys);

        reduce(&mut state, AppEvent::Action(Action::NextPane));
        assert_eq!(state.focus, Focus::Value);

        reduce(&mut state, AppEvent::Action(Action::PrevPane));
        assert_eq!(state.focus, Focus::Keys);
    }

    #[test]
    fn enter_walks_types_to_keys_to_value() {
        let mut state = state();
        reduce(&mut state, AppEvent::Action(Action::Activate));
        assert_eq!(state.focus, Focus::Keys);
        reduce(&mut state, AppEvent::Action(Action::Activate));
        assert_eq!(state.focus, Focus::Value);
        reduce(&mut state, AppEvent::Action(Action::Activate));
        assert_eq!(state.focus, Focus::Value);
    }

    #[test]
    fn scan_results_are_bucketed_per_type() {
        let mut state = state();
        reduce(
            &mut state,
            page(
                42,
                false,
                vec![
                    (DataType::String, "a".to_string()),
                    (DataType::Hash, "b".to_string()),
                    (DataType::String, "c".to_string()),
                ],
            ),
        );

        let strings = state.scan.bucket(DataType::String).unwrap();
        assert_eq!(strings.count, 2);
        assert_eq!(strings.keys, vec!["a", "c"]);

        let hashes = state.scan.bucket(DataType::Hash).unwrap();
        assert_eq!(hashes.count, 1);

        assert!(!state.scan.done);
        assert!(state.scan.cursor == 42);
    }

    #[test]
    fn filter_mode_accepts_letters_bound_in_normal_mode() {
        let mut state = state();
        state.mode = Mode::Filter;

        for letter in ['j', 'k', 'r', 'q', 'g', '/', '?'] {
            let key = KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE);
            reduce(&mut state, AppEvent::Key(key));
        }

        assert_eq!(state.filter, "jkrqg/?");
    }

    #[test]
    fn filter_mode_still_honours_control_keys() {
        let mut state = state();
        state.mode = Mode::Filter;

        reduce(
            &mut state,
            AppEvent::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
        );
        assert_eq!(state.mode, Mode::Normal);

        state.mode = Mode::Filter;
        reduce(
            &mut state,
            AppEvent::Key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
        );
        assert!(state.should_quit);
    }

    #[test]
    fn scan_ignores_pages_from_a_superseded_run() {
        let mut state = state();
        state.scan.id = 7;
        reduce(
            &mut state,
            page(1, true, vec![(DataType::String, "a".to_string())]),
        );
        assert!(state.scan.buckets.is_empty());
    }

    #[test]
    fn key_selection_is_clamped_when_new_keys_arrive() {
        let mut state = state();
        state.lists.keys.select(Some(2));
        reduce(
            &mut state,
            page(1, true, vec![(DataType::String, "only".to_string())]),
        );
        assert_eq!(state.lists.keys.selected(), Some(0));
    }

    #[test]
    fn cursor_movement_stops_at_the_boundaries() {
        let mut state = state();
        reduce(
            &mut state,
            page(0, true, vec![(DataType::String, "a".to_string())]),
        );

        reduce(&mut state, AppEvent::Action(Action::Up));
        assert_eq!(state.lists.types.selected(), Some(0));

        reduce(&mut state, AppEvent::Action(Action::Bottom));
        assert_eq!(state.lists.types.selected(), Some(DataType::ALL.len() - 1));

        reduce(&mut state, AppEvent::Action(Action::Down));
        assert_eq!(state.lists.types.selected(), Some(DataType::ALL.len() - 1));
    }

    #[test]
    fn changing_type_resets_the_key_selection() {
        let mut state = state();
        reduce(
            &mut state,
            page(
                0,
                true,
                vec![
                    (DataType::String, "a".to_string()),
                    (DataType::String, "b".to_string()),
                    (DataType::List, "only-list".to_string()),
                ],
            ),
        );
        state.lists.keys.select(Some(1));
        assert_eq!(state.selected_key().as_deref(), Some("b"));

        reduce(&mut state, AppEvent::Action(Action::Down));

        assert_eq!(state.selected_type(), DataType::List);
        assert_eq!(state.lists.keys.selected(), Some(0));
        assert_eq!(state.selected_key().as_deref(), Some("only-list"));
    }

    #[test]
    fn filter_mode_edits_text_and_escape_restores_it() {
        let mut state = state();
        state.filter = "user:*".to_string();
        state.scan.filter = "user:*".to_string();
        state.mode = Mode::Filter;

        reduce(&mut state, AppEvent::Action(Action::Char('x')));
        reduce(&mut state, AppEvent::Action(Action::Char('y')));
        assert_eq!(state.filter, "user:*xy");

        reduce(&mut state, AppEvent::Action(Action::Backspace));
        assert_eq!(state.filter, "user:*x");

        reduce(&mut state, AppEvent::Action(Action::Cancel));
        assert_eq!(state.mode, Mode::Normal);
        assert_eq!(state.filter, "user:*");
    }

    #[test]
    fn filter_characters_do_not_leak_into_normal_mode() {
        let mut state = state();
        reduce(&mut state, AppEvent::Action(Action::Char('z')));
        assert_eq!(state.filter, "");
    }

    #[test]
    fn toggle_help_flips_the_overlay() {
        let mut state = state();
        reduce(&mut state, AppEvent::Action(Action::ToggleHelp));
        assert!(state.show_help);
        reduce(&mut state, AppEvent::Action(Action::Cancel));
        assert!(!state.show_help);
    }

    #[test]
    fn any_key_dismisses_the_help_overlay() {
        let mut state = state();
        state.show_help = true;

        let commands = reduce(&mut state, AppEvent::Action(Action::Down));

        assert!(!state.show_help);
        assert!(
            commands.is_empty(),
            "the key must not reach the app underneath"
        );
    }

    #[test]
    fn ticks_retry_the_connection_while_offline() {
        let mut state = state();
        // on_tick increments before checking, so land exactly on the interval.
        state.tick = RECONNECT_TICKS - 1;

        let first = reduce(&mut state, AppEvent::Tick);
        assert!(matches!(first.as_slice(), [Command::Connect]));
        assert!(state.connecting);

        let second = reduce(&mut state, AppEvent::Tick);
        assert!(second.is_empty(), "must not stack connection attempts");
    }

    #[test]
    fn reconnect_attempts_are_spaced_out() {
        let mut state = state();
        state.connecting = false;
        state.tick = 1;

        assert!(
            reduce(&mut state, AppEvent::Tick).is_empty(),
            "retrying on every tick would hammer a dead server"
        );

        state.tick = RECONNECT_TICKS - 1;
        assert!(matches!(
            reduce(&mut state, AppEvent::Tick).as_slice(),
            [Command::Connect]
        ));
    }

    #[tokio::test]
    async fn a_failed_heartbeat_drops_the_connection_so_it_is_rebuilt() {
        let mut state = state();
        state.server.connected = true;
        state.heartbeat_in_flight = true;
        state.conn = Some(
            redis::aio::ConnectionManager::new_lazy_with_config(
                redis::Client::open("redis://127.0.0.1:6379").unwrap(),
                redis::aio::ConnectionManagerConfig::new(),
            )
            .expect("lazy connection needs no server"),
        );

        reduce(
            &mut state,
            AppEvent::HeartbeatFailed {
                error: "broken pipe".to_string(),
            },
        );

        assert!(
            state.conn.is_none(),
            "a dead connection must be discarded, not reused"
        );
    }

    #[test]
    fn heartbeat_fires_every_eighth_tick_and_never_stacks() {
        let mut state = state();

        for _ in 0..HEARTBEAT_TICKS - 1 {
            state.tick += 1;
            assert!(
                !heartbeat_due(&mut state),
                "tick {} is before the interval",
                state.tick
            );
        }

        state.tick += 1;
        assert!(
            heartbeat_due(&mut state),
            "tick {HEARTBEAT_TICKS} should fire a heartbeat"
        );
        assert!(
            !heartbeat_due(&mut state),
            "a second probe must wait for the first to finish"
        );

        state.heartbeat_in_flight = false;
        state.tick += 1;
        assert!(!heartbeat_due(&mut state), "next tick is not due");
        state.tick += HEARTBEAT_TICKS - 1;
        assert!(
            heartbeat_due(&mut state),
            "the cadence resumes on tick {}",
            state.tick
        );
    }

    #[test]
    fn a_failed_heartbeat_marks_the_server_offline() {
        let mut state = state();
        state.server.connected = true;
        state.heartbeat_in_flight = true;

        reduce(
            &mut state,
            AppEvent::HeartbeatFailed {
                error: "broken pipe".to_string(),
            },
        );

        assert!(!state.server.connected);
        assert!(!state.heartbeat_in_flight);
        assert!(
            state
                .status
                .expect("status")
                .text
                .contains("connection lost")
        );
    }

    #[test]
    fn a_recovered_heartbeat_keeps_going_without_rescanning() {
        let mut state = state();
        state.server.connected = true;
        state.heartbeat_in_flight = true;

        let commands = reduce(&mut state, AppEvent::HeartbeatOk);

        assert!(commands.is_empty(), "a healthy link needs no rescan");
        assert!(state.server.connected);
        assert!(!state.heartbeat_in_flight);
    }

    #[test]
    fn connect_failure_surfaces_in_the_status_bar() {
        let mut state = state();
        reduce(
            &mut state,
            AppEvent::ConnectFailed {
                error: "refused".to_string(),
            },
        );

        assert!(!state.server.connected);
        assert!(!state.connecting);
        let status = state.status.expect("status message");
        assert!(status.is_error);
        assert!(status.text.contains("refused"));
    }

    #[test]
    fn value_results_for_other_keys_are_discarded() {
        let mut state = state();
        state.value.id = 4;

        reduce(
            &mut state,
            AppEvent::ValueLoaded {
                id: 3,
                view: crate::state::ValueView::Scalar {
                    text: "stale".to_string(),
                    bytes: 5,
                    binary: false,
                },
            },
        );
        assert!(state.value.view.is_none());

        reduce(
            &mut state,
            AppEvent::ValueLoaded {
                id: 4,
                view: crate::state::ValueView::Scalar {
                    text: "fresh".to_string(),
                    bytes: 5,
                    binary: false,
                },
            },
        );
        assert!(matches!(
            state.value.view,
            Some(crate::state::ValueView::Scalar { .. })
        ));
    }

    #[test]
    fn rescan_is_a_no_op_while_disconnected() {
        let mut state = state();
        assert!(reduce(&mut state, AppEvent::Action(Action::Refresh)).is_empty());
    }
}
