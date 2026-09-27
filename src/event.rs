use std::time::Duration;

use futures::StreamExt;
use ratatui::crossterm::event::{
    Event as CrosstermEvent, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use redis::aio::ConnectionManager;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::command::Command;
use crate::state::{KeyDetail, ValueView};
use crate::store::{ScanPage, ServerInfo};
pub const TICK_RATE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Cancel,
    NextPane,
    PrevPane,
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Activate,
    ScrollLeft,
    ScrollRight,
    BeginFilter,
    Refresh,
    ToggleHelp,
    Backspace,
    Char(char),
}

impl From<KeyEvent> for Action {
    fn from(key: KeyEvent) -> Self {
        if key.kind == KeyEventKind::Release {
            return Action::None;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => Action::Quit,
            KeyCode::Char('c') | KeyCode::Char('q') => Action::Quit,
            KeyCode::Esc => Action::Cancel,
            KeyCode::Tab => Action::NextPane,
            KeyCode::BackTab => Action::PrevPane,
            KeyCode::Down | KeyCode::Char('j') => Action::Down,
            KeyCode::Up | KeyCode::Char('k') => Action::Up,
            KeyCode::PageDown => Action::PageDown,
            KeyCode::PageUp => Action::PageUp,
            KeyCode::Home | KeyCode::Char('g') => Action::Top,
            KeyCode::End | KeyCode::Char('G') => Action::Bottom,
            KeyCode::Enter => Action::Activate,
            KeyCode::Left => Action::ScrollLeft,
            KeyCode::Right => Action::ScrollRight,
            KeyCode::Char('/') => Action::BeginFilter,
            KeyCode::Char('r') => Action::Refresh,
            KeyCode::Char('?') => Action::ToggleHelp,
            KeyCode::Backspace => Action::Backspace,
            KeyCode::Char(c) => Action::Char(c),
            _ => Action::None,
        }
    }
}

/// While editing the filter, almost every key is text. Only the handful of keys
/// that must still work are given a meaning.
pub fn translate_filter_key(key: KeyEvent) -> Action {
    if key.kind == KeyEventKind::Release {
        return Action::None;
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('c') if ctrl => Action::Quit,
        KeyCode::Char('c') => Action::Quit,
        KeyCode::Esc => Action::Cancel,
        KeyCode::Enter => Action::Activate,
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Tab => Action::None,
        KeyCode::Char(c) => Action::Char(c),
        _ => Action::None,
    }
}

#[derive(Debug)]
pub enum AppEvent {
    Key(KeyEvent),
    Action(Action),
    Resize,
    Tick,
    InputError(String),
    Connected {
        conn: ConnectionManager,
        info: ServerInfo,
    },
    ConnectFailed {
        error: String,
    },
    ScanProgress {
        id: u64,
        page: ScanPage,
    },
    ScanFailed {
        id: u64,
        error: String,
    },
    ValueLoaded {
        id: u64,
        view: ValueView,
    },
    ValueFailed {
        id: u64,
        error: String,
    },
    DetailLoaded {
        id: u64,
        detail: KeyDetail,
    },
    DetailFailed {
        id: u64,
        error: String,
    },
    HeartbeatOk,
    HeartbeatFailed {
        error: String,
    },
}

pub struct Events {
    stream: EventStream,
    worker_rx: UnboundedReceiver<AppEvent>,
}

impl Events {
    pub fn new(worker_rx: UnboundedReceiver<AppEvent>) -> Self {
        Self {
            stream: EventStream::new(),
            worker_rx,
        }
    }

    pub async fn next(&mut self) -> AppEvent {
        tokio::select! {
            event = self.stream.next() => match event {
                Some(Ok(CrosstermEvent::Key(key))) => AppEvent::Key(key),
                Some(Ok(CrosstermEvent::Resize(_, _))) => AppEvent::Resize,
                Some(Ok(_)) => AppEvent::Action(Action::None),
                Some(Err(error)) => AppEvent::InputError(error.to_string()),
                None => AppEvent::InputError("input stream closed".to_string()),
            },
            event = self.worker_rx.recv() => event.unwrap_or(AppEvent::InputError("worker channel closed".to_string())),
            _ = tokio::time::sleep(TICK_RATE) => AppEvent::Tick,
        }
    }
}

pub fn dispatch<R>(command: Command, repo: R, tx: UnboundedSender<AppEvent>)
where
    R: crate::store::RedisRepository + Clone + Send + Sync + 'static,
{
    tokio::spawn(async move {
        let event = match command {
            Command::Connect => match repo.connect().await {
                Ok((conn, info)) => AppEvent::Connected { conn, info },
                Err(error) => AppEvent::ConnectFailed {
                    error: error.to_string(),
                },
            },
            Command::Scan {
                id,
                conn,
                cursor,
                filter,
                count,
            } => match repo.scan(conn, cursor, &filter, count).await {
                Ok(page) => AppEvent::ScanProgress { id, page },
                Err(error) => AppEvent::ScanFailed {
                    id,
                    error: error.to_string(),
                },
            },
            Command::Value {
                id,
                conn,
                key,
                data_type,
            } => match repo.fetch_value(conn, &key, data_type).await {
                Ok(view) => AppEvent::ValueLoaded { id, view },
                Err(error) => AppEvent::ValueFailed {
                    id,
                    error: error.to_string(),
                },
            },
            Command::Detail { id, conn, key } => match repo.fetch_detail(conn, &key).await {
                Ok(detail) => AppEvent::DetailLoaded { id, detail },
                Err(error) => AppEvent::DetailFailed {
                    id,
                    error: error.to_string(),
                },
            },
            Command::Heartbeat { conn } => match repo.ping(conn).await {
                Ok(()) => AppEvent::HeartbeatOk,
                Err(error) => AppEvent::HeartbeatFailed {
                    error: error.to_string(),
                },
            },
        };
        let _ = tx.send(event);
    });
}
