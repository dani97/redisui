use redis::aio::ConnectionManager;

use crate::state::DataType;

#[derive(Debug)]
pub enum Command {
    Connect,
    Scan {
        id: u64,
        conn: ConnectionManager,
        cursor: u64,
        filter: String,
        count: usize,
    },
    Value {
        id: u64,
        conn: ConnectionManager,
        key: String,
        data_type: DataType,
    },
    Detail {
        id: u64,
        conn: ConnectionManager,
        key: String,
    },
    /// Cheap liveness probe so the UI notices a server that went away.
    Heartbeat {
        conn: ConnectionManager,
    },
}
