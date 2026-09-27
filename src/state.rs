use std::collections::BTreeMap;

use ratatui::widgets::ListState;

use crate::store::ServerInfo;

pub const KEYS_PER_TYPE_LIMIT: usize = 2_000;
pub const SCAN_COUNT: usize = 200;
pub const FILTERED_SCAN_COUNT: usize = 1_000;
pub const VALUE_ROW_LIMIT: usize = 1_000;
pub const STRING_PREVIEW_BYTES: usize = 8_192;

/// Ticks between liveness probes, at ~4 ticks/sec this is roughly every 2 seconds.
pub const HEARTBEAT_TICKS: u64 = 8;

/// Ticks between reconnection attempts once the link is down.
pub const RECONNECT_TICKS: u64 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DataType {
    String,
    List,
    Hash,
    Set,
    SortedSet,
}

impl DataType {
    pub const ALL: [DataType; 5] = [
        DataType::String,
        DataType::List,
        DataType::Hash,
        DataType::Set,
        DataType::SortedSet,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DataType::String => "strings",
            DataType::List => "arrays",
            DataType::Hash => "hashes",
            DataType::Set => "sets",
            DataType::SortedSet => "sorted sets",
        }
    }

    pub const WIDTH: usize = 12;

    pub fn from_redis_type(name: &str) -> Option<Self> {
        match name {
            "string" => Some(DataType::String),
            "list" => Some(DataType::List),
            "hash" => Some(DataType::Hash),
            "set" => Some(DataType::Set),
            "zset" => Some(DataType::SortedSet),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct TypeBucket {
    pub count: usize,
    pub keys: Vec<String>,
}

impl TypeBucket {
    pub fn truncated(&self) -> bool {
        self.keys.len() >= KEYS_PER_TYPE_LIMIT
    }
}

#[derive(Debug, Default)]
pub struct ScanState {
    pub id: u64,
    pub cursor: u64,
    pub in_flight: bool,
    pub done: bool,
    pub total_keys: u64,
    pub filter: String,
    pub buckets: BTreeMap<DataType, TypeBucket>,
}

impl ScanState {
    pub fn bucket(&self, data_type: DataType) -> Option<&TypeBucket> {
        self.buckets.get(&data_type)
    }
}

#[derive(Debug, Default)]
pub struct ServerState {
    pub info: Option<ServerInfo>,
    pub connected: bool,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub enum ValueView {
    Scalar {
        text: String,
        bytes: usize,
        binary: bool,
    },
    Rows {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
        total: usize,
    },
}

#[derive(Debug, Default)]
pub struct ValueState {
    pub id: u64,
    pub key: String,
    pub loading: bool,
    pub view: Option<ValueView>,
}

impl ValueState {
    pub fn begin(&mut self, id: u64, key: String) {
        *self = ValueState {
            id,
            key,
            loading: true,
            view: None,
        };
    }

    pub fn row_count(&self) -> usize {
        match &self.view {
            Some(ValueView::Rows { rows, .. }) => rows.len(),
            Some(ValueView::Scalar { text, .. }) => text.lines().count(),
            None => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ttl {
    Persistent,
    Expiring(i64),
    Unknown,
}

impl Ttl {
    pub fn label(self) -> String {
        match self {
            Ttl::Persistent => "no expiry".to_string(),
            Ttl::Expiring(secs) => format!("{}s", secs.max(0)),
            Ttl::Unknown => "key gone".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyDetail {
    pub data_type: DataType,
    pub ttl: Ttl,
    pub encoding: String,
    pub memory_bytes: Option<u64>,
}

#[derive(Debug, Default)]
pub struct DetailState {
    pub id: u64,
    pub key: String,
    pub loading: bool,
    pub detail: Option<KeyDetail>,
}

impl DetailState {
    pub fn begin(&mut self, id: u64, key: String) {
        *self = DetailState {
            id,
            key,
            loading: true,
            detail: None,
        };
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Types,
    Keys,
    Value,
    Detail,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Focus::Types => Focus::Keys,
            Focus::Keys => Focus::Value,
            Focus::Value => Focus::Detail,
            Focus::Detail => Focus::Types,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Focus::Types => Focus::Detail,
            Focus::Keys => Focus::Types,
            Focus::Value => Focus::Keys,
            Focus::Detail => Focus::Value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Filter,
}

#[derive(Debug, Default)]
pub struct Lists {
    pub types: ListState,
    pub keys: ListState,
    pub value: ListState,
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub text: String,
    pub is_error: bool,
}

#[derive(Debug)]
pub struct AppState {
    pub session: crate::store::Session,
    pub conn: Option<redis::aio::ConnectionManager>,
    pub server: ServerState,
    pub scan: ScanState,
    pub value: ValueState,
    pub detail: DetailState,
    pub lists: Lists,
    pub focus: Focus,
    pub mode: Mode,
    pub filter: String,
    pub show_help: bool,
    pub should_quit: bool,
    pub connecting: bool,
    pub heartbeat_in_flight: bool,
    pub status: Option<StatusMessage>,
    pub tick: u64,
    next_id: u64,
}

impl AppState {
    pub fn new(session: crate::store::Session) -> Self {
        Self {
            session,
            conn: None,
            server: ServerState::default(),
            scan: ScanState::default(),
            value: ValueState::default(),
            detail: DetailState::default(),
            lists: Lists::default(),
            focus: Focus::Types,
            mode: Mode::Normal,
            filter: String::new(),
            show_help: false,
            should_quit: false,
            connecting: false,
            heartbeat_in_flight: false,
            status: None,
            tick: 0,
            next_id: 0,
        }
    }

    pub fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn selected_type(&self) -> DataType {
        let index = self.lists.types.selected().unwrap_or(0);
        DataType::ALL[index.min(DataType::ALL.len() - 1)]
    }

    pub fn visible_keys(&self) -> &[String] {
        self.scan
            .bucket(self.selected_type())
            .map(|bucket| bucket.keys.as_slice())
            .unwrap_or(&[])
    }

    pub fn selected_key(&self) -> Option<String> {
        let index = self.lists.keys.selected().unwrap_or(0);
        self.visible_keys().get(index).cloned()
    }

    pub fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status = Some(StatusMessage {
            text: text.into(),
            is_error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_redis_type_names() {
        assert_eq!(DataType::from_redis_type("string"), Some(DataType::String));
        assert_eq!(DataType::from_redis_type("zset"), Some(DataType::SortedSet));
        assert_eq!(DataType::from_redis_type("ReJSON-RL"), None);
        assert_eq!(DataType::from_redis_type("none"), None);
    }

    #[test]
    fn reports_ttl_labels() {
        assert_eq!(Ttl::Persistent.label(), "no expiry");
        assert_eq!(Ttl::Expiring(-1).label(), "0s");
        assert_eq!(Ttl::Expiring(30).label(), "30s");
    }

    #[test]
    fn cycles_focus_in_both_directions() {
        assert_eq!(Focus::Types.next(), Focus::Keys);
        assert_eq!(Focus::Detail.next(), Focus::Types);
        assert_eq!(Focus::Types.prev(), Focus::Detail);
        assert_eq!(Focus::Keys.prev(), Focus::Types);
    }
}
